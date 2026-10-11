# Offline Python tooling

Heavy commands run sequentially under one allocation of at most 12 CPU-heavy
workers. `--workers N` selects a count within the caller's `PLACES_TOOL_WORKERS`
allocation, available CPUs and independent job count. `--workers 1` executes the
same algorithms directly (for native Blender, it caps the native CPU allocation). Do not overlap these commands with Cargo builds or
other expensive jobs. Core validators, codecs and writers use the Python standard
library and repository modules. Explicit artwork authoring and Winter derivatives
also use Pillow; the named Blender workflows require Blender. These tools require
Python 3.10 or newer. Install only the dependency needed for the selected workflow.

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

The player's level preparation shares that budget: catalog texture decode runs
up to four concurrent workers, clamped by the `--workers` value the compiler
stores as the process preparation budget, so `--workers 1` keeps a tool run
serial. The runtime never exceeds the one shared allocation.

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
# Compiled map packages: the explicit edit -> compile -> launch workflow.
cargo build --release
./target/release/places-compile build levels/places_demo.json --out assets/levels/places_demo.placesmap
./target/release/places-compile build levels/model_zoo.json --out assets/levels/model_zoo.placesmap --workers 8
sh tools/build-maps.sh  # maintained lineup, explicit assets/levels/ destinations
./target/release/places-compile validate assets/levels/places_demo.placesmap
./target/release/places-compile inspect assets/levels/places_demo.placesmap
./target/release/places-compile verify levels/places_demo.json \
    --package assets/levels/places_demo.placesmap
PLACES_LEVEL=places_demo ./target/release/places

# Discovery only: the playable rows the Level Select menu would show, without a
# window or a GPU. One `id<TAB>source<TAB>path` row per compiled package; the
# JSON authoring sources live under levels/; only compiled packages are playable.
./target/release/places --list-levels

python3 -m unittest tests.test_tool_execution tests.test_zoo_generator
python3 tools/props/preview.py --all --workers 8 --out target/previews
python3 tools/textures/build.py --check
python3 tools/textures/author_arcade.py --check  # saved masters/native parity
python3 tools/levels/build_retro_arcade.py --check
python3 tools/textures/build.py --workers 4 --only core:tex_pool_tile_deck_01
python3 tools/props/build.py --workers 4 --only home:fork home:bowl
python3 tools/props/animate_spooner_man.py --workers 6
python3 tools/props/repair_geometry.py --workers 8 --report target/geometry.json
python3 tools/props/resize_embedded_textures.py target/import.glb --size 256 --workers 8
python3 tools/textures/seam_repair.py --workers 4 --check assets/environments/pool/textures/walls/*.png
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

## Asset authoring

Read the [asset specification](ASSET_SPECIFICATION.md) before changing imagery or
model texture contracts, and the [map authoring guide](MAP_AUTHORING_GUIDE.md)
before changing map assets or placement. The [asset library](../assets/README.md)
explains current folders; [environment](assets/ENVIRONMENTS.md) and
[entity](assets/ENTITIES.md) profiles preserve kit-specific layouts and mounting.
Concept sheets and prompts in `assets/references/` retain their original bytes;
curated screenshots and reports record specific reviews rather than live gate
status.

### Source ownership and export

All runtime texture imagery is a committed PNG under `assets/`. Normal model
builders read those sources and embed the native sheet; the player reads saved
PNG/GLB resources. An offline authoring script may paint a replacement deliberately,
but it is not a runtime texture generator. Keep approved art and rigs intact while
building a proposed change in scratch and comparing geometry, images and metadata.
The following owners have distinct behavior:

| Owner | Scope and publication contract |
| --- | --- |
| `tools/props/build.py` | Catalog placeables with registered part-module `PROPS` constructors; writes the catalog's model paths. Normal builds skip foreign skinned/animated GLBs. |
| `tools/textures/build.py` | Merges each art module's `ART` registry by logical ID, rejects duplicate declarations, and exports selected manifest sheets. Office, Beach and Aero readers load committed art; some older modules remain offline painters. |
| `tools/props/build_office_textures.py` | Exact standard-library box-filter derivatives of five opaque 1024² masters to 256² native atlases; `--check` compares bytes without writes. |
| `tools/textures/author_home.py`, `author_pool.py`, `author_outdoors.py` | Explicit Pillow source/native artwork exporters. Running these intentionally rewrites their selected kit artwork; they do not provide a generic read-only check mode. |
| `tools/props/build_winter_textures.py` | Pillow-based source derivatives and seam preservation for Winter; `--check` verifies existing output. |
| `tools/textures/beach_art.py`, `frutiger_aero_art.py` | Explicit Pillow `--author` writes source/runtime PNGs; `--check` recreates outputs in memory and compares exact bytes. Normal texture exports read saved PNGs. |
| `tools/entities/build_beach_{seagull,crab,fish}.py` | `--author-textures` explicitly paints retained masters/natives; normal export reads them. `--check` compares a temporary deterministic GLB and samples its animation/geometry. |
| `tools/props/animate_spooner_man.py` | Preserves the manual cat mesh/art/rig while baking clips; `--check` validates existing animation. `--repair-skin` explicitly permits the documented weight rebind. |
| `tools/entities/build_mannequin.py`, `build_rat.py`, `build_skeleton.py` | Original mesh/pose construction sources. Current GLBs include later refinements; mannequin walk/run and rat death are not recreated by the original constructors. Review scratch exports before replacing committed models. Rat union requires Blender. |
| `tools/entities/author_halloween_assets.py` | Idempotent material/primitive contract maintenance on external authored rigs; `--check` verifies them. Preserves positions, UVs, vertex colours, skin and animation. |

The texture dimension guard skips an existing sheet when its dimensions differ
from a painter's output unless `--force` is given. It does not protect same-size
art from replacement. The prop guard checks skins/animations and the
`asset.extras.places_props_toolkit = 1` marker: toolkit-authored animated props
can rebuild; foreign rigs are skipped. `--force` opts out of that protection.
Use the actual owner rather than a broad forced rebuild.

For a new static prop, add its PNG first, define a constructor in `parts/`, add
its logical ID to that module's `PROPS` table and register the module in
`parts/__init__.py`. Add the catalog file entry and measured size. `parts/refreshed.py::load_atlas_from` accepts an explicit PNG path and named
fitted regions; `load_atlas` is the shorthand for `assets/shared/props/`.
The helper accepts square 32/64/128/256 sheets. Opaque is the default;
`keep_alpha=True` preserves alpha for a matching MASK material. Preserve atlas
region boundaries and UV insets rather than stretching art to a new layout.

`glb.write_glb` preserves the simple one-node/one-primitive output when no
extended fields are supplied. Its extended path accepts material slots, named
submesh boundaries, node transforms and LINEAR/STEP clip channels; material
runs become separate glTF primitives and attributes are compacted per primitive.
`PropBuilder.material` supports MASK/cutoff, BLEND/scalar opacity, roughness,
metallic response and emission. Scalar opacity must be finite in 0..1 and uses
BLEND. `use_texture=False` selects the committed white runtime texture, useful
for snow without changing a foliage atlas. Material settings do not replace
required PNG imagery or native visual acceptance.

### What each check establishes

```sh
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 tools/assets/audit.py --workers 8 --out target/asset-audit/inventory.json
python3 tools/entities/validate_entities.py --workers 8
python3 tools/entities/check_clip_boundaries.py
```

Catalog validation checks registry and map contracts. Texture `--check` verifies
file-backed surface, decal and fixture PNG signatures, IHDR/IEND, dimensions and
budgets; it also warns on catalog/manifest drift. It does not decode every pixel
or test seams, fitted orientation or source/embedded parity. Preferred sizes and
power-of-two warnings are distinct from mandatory sky aspect/POT checks. Prop
`--check` reads current GLB structure, reports bounds/triangle counts and checks
primary embedded-image dimensions plus pack texture memory. Constructor export and
Rust asset tests enforce additional size/origin/UV/geometry contracts. It does not
compare every model with its constructor or prove runtime rendering. The inventory audit reports byte/pixel duplicates,
source coverage and file metadata for review; intentional family sources remain
separate even when their pixels match.

Seam checks and repairs are separate, explicit operations:

```sh
python3 tools/textures/seam_repair.py --report assets/environments/pool/textures/walls/*.png
python3 tools/textures/seam_repair.py --check assets/environments/pool/textures/walls/*.png
python3 tools/textures/normalize_sky_seam.py --check assets/environments/winter/textures/sky/*.png
```

`seam_repair.py --repair` modifies named images and exposes low-frequency,
residual-band and blur parameters. Preserve alpha and verify the asset's tiling
contract before applying it. `normalize_sky_seam.py --repair` changes RGB only
within narrow longitude-edge strips, preserving alpha, dimensions, colour type,
ancillary chunks and the interior. It joins U, never the clamped poles.

`repair_geometry.py` defaults to a report and only scans its explicit `NAMES`
set of reviewed domestic, Office and Pool models. `--apply` opts into its proposed
repairs after original-hash preflight. It is not a universal capper: open trays,
shade interiors, rail ends and intentional assembly contacts need their own
geometry rules. Recheck UVs, materials, bounds, collision and native silhouettes
after any topology change. The [repair report](reports/geometry-repair.md)
records the original scope and reviewed exceptions.

`preview.py` produces scratch bind-pose software images/contact sheets. It uses
textures, vertex colours, material factors and MASK cutouts. BLEND triangles are
sorted in camera space, test opaque depth and blend without writing depth; this
inspection sorter differs from the native renderer. It does not reproduce native reflection, transmission,
emissive lighting or skin animation. Use it for construction/UV review, then
run the [native verification workflow](VERIFICATION.md) for rendering acceptance.
For posed rigs, Blender contact sheets reimport exported GLBs and sample clips;
these are still a separate check from the player's animation and illumination.

### Entity clip metadata

`tools/entities/rig.py` supplies rig math, the skin/animation GLB writer and a
structural `--check GLB`. Each clip's v2 declaration lives in
`asset.extras.places_entity_clips`, with this shape:

```json
{"version":2,"generator":"tools/entities/build_beach_crab.py","clips":[
  {"name":"walk","duration":0.8,"loop":true,"kind":"walk",
   "reference_speed_mps":0.16062456434191058}
]}
```

Preserve joint ordering, inverse binds, fitted artwork and clip declarations when
editing an existing rig. Measure a ground stride from the exported skinned mesh
before setting its speed. Duration-zero pose holds have no travel speed; airborne
and swimming clips use their own kinds. The current model record is authoritative
when a historical report or original constructor declares older values.
`validate_entities.py` samples skin/contact/bounds; `check_clip_boundaries.py`
checks looping endpoints and declared transitions, with one-shot gestures excluded
from loop closure. Neither substitutes for native playback, collision or lighting.

## Lighting, probes and quality variants the compiler prepares

`places-compile build` runs the offline transport solver
(`src/lighting/transport.rs`) for every `medium`/`full` variant: direct
emitter sampling with soft shadows, one or two visibility-tested diffuse
bounces, a moment per-texel encoding (an irradiance mean plus the signed
vector sum of the per-channel first moments), and a prepared
irradiance field for moving objects (solved from the non-switchable emitters;
a switchable fixture's contribution is prepared only as its extra atlas layer
pair). The result is packaged as linear
HDR KTX2 lightmaps (`R16G16B16A16_SFLOAT`, two layers per page plus one pair
per switchable fixture), a `PLPF` irradiance-field record and prefiltered
reflection cubemap mip chains. The `off` variant ships the historical
vertex-lit mesh and no atlas.

Quality variants differ in prepared data, never in runtime work: Medium runs
one bounce with two emitter taps per axis at 12 texels/m; Full runs two
bounces with three taps at 16 texels/m. `--workers N` sets the shared CPU
budget for the solve (default from the host's available parallelism, clamped
to 12; `PLACES_TOOL_WORKERS` is the environment fallback and the flag wins).
`--workers 1` is the serial reference path and is tested to produce identical
values to the parallel path.

## Navigation the compiler prepares

`places-compile build` also bakes the navigation mesh
(`src/nav/bake.rs`) and stores it as one `blobs/<sha>.navigation` record per
package. The bake rasterises the level's own walkable surfaces (rooms, floor
regions, ramps, stairs) against the same collision boxes the player collides
with, producing:

* one **agent class** per distinct `nav_agent` body in the level (plus the
  reference humanoid), each with a walkable bitmask and connected-region
  labels; clearance, headroom, step and slope are evaluated at bake time;
* per-cell surface height, headroom and door-portal membership;
* one portal per authored door leaf, so a closed or locked door blocks a route
  and an open one passes without any rebake.

Independent row tiles are baked in parallel with a private collision index per
worker (at most the shared worker budget); the merge is in row order, so a
parallel bake is byte-identical to `--workers 1` (covered by
`nav::tests::parallel_and_serial_bakes_are_identical`). The bake also warns at
build time, naming any `nav_agent` entity or spawn point with no navigable
cell; the command output lists those warnings.

The runtime never bakes. It decodes, validates and queries the record; a
missing or malformed record fails the player load by name.

Illumination is a separate stage: the manifest records a
`lighting_fingerprint` over the level's geometry, materials and lights with
every navigation/AI component removed. An edit that only changes an encounter
or an AI behavior keeps that key and the compiler reuses the previous package's
prepared geometry, lightmaps, probes and collision, rebuilding only the
semantic and navigation records (`loader::tests::an_ai_only_edit_reuses_the_prepared_lighting`).

## Tool inventory

A = CPU work partitioned at the listed consumer; B = already parallel; C = I/O or
external orchestration; D = bounded/inherently serial; E = obsolete, unavailable or failing prerequisites.
Libraries return in-memory data; their consumers publish the listed outputs.
Dependencies: S = standard library, P = prop toolkit, T = texture toolkit,
E = entity toolkit, B = Blender, I = Pillow. The table distinguishes core checks
from deliberate source-art exporters with additional dependencies.

| Entry point | Purpose / consumers | Dependencies / output | Class and execution |
| --- | --- | --- | --- |
| `src/bin/places-compile.rs` | Offline map compiler CLI (build/build-collection/validate/inspect/verify) | S/Places library; `.placesmap` packages | A per atlas fill (1..=3), plus one headless GPU probe capture per variant; serial across sources |
| `tests/test_compiled_build.py` | Packaged native windowed smoke | S/Places; scratch runtime output | C; sequential native subprocesses |
| `tests/test_bench_metrics.py` | Trace interval and committed-world metric known answers | S/bench helpers; scratch JSON | D; tiny deterministic fixtures |
| `tests/test_lightmap_harness.py` | Capture environment and missing-output rejection | S/mock subprocess; scratch paths | D; no native game |
| `tests/test_package.py` | Repository/asset regressions; verify.sh | S/validators; scratch/stdout | D; bounded assertions |
| `tests/test_tool_execution.py` | Known answers, failure/cancellation, ordering and budgets | S/tools; scratch/stdout | B; fast spawn regressions |
| `tests/test_wgpu_bootstrap.py` | Native renderer lifecycle/quality regressions | S/Places; scratch captures | C; sequential native subprocesses |
| `tests/test_zoo_generator.py` | Catalog growth/determinism regressions | S/zoo; scratch/stdout | B; serial and spawn inspection |
| `tools/assets/audit.py` | File inventory, source/embedded coverage and duplicate diagnostics | S/P; inventory JSON under target by default | A decode batches; parent report |
| `tools/assets/validate.py` | Catalog/map schema checks; gate/package tests | S; stdout | D; fast structural validation |
| `tools/bench/bench_local.py` | Native game repeated timing; evidence | S/Places; JSON/logs | C; serial to avoid measurement interference |
| `tools/bench/check_holes.py` | Capture dark-pixel audit | S; stdout/JSON | A per-image spawn |
| `tools/bench/check_tool_parallelism.py` | Original/serial/parallel scratch benchmark | S/tools; scratch hashes/logs/times | C; serial orchestration of bounded heavy jobs |
| `tools/bench/compare_baseline.py` | Frozen baseline byte comparisons | S; stdout | C; file reads and byte comparison |
| `tools/bench/compare_captures.py` | Per-view capture differences | S; stdout | A per-pair spawn |
| `tools/bench/lightmap_report.py` | Native capture and lighting telemetry | S/Places; PNG/log/report | C; serial native game |
| `tools/bench/loading.py` | Already-built native startup and application-cache comparison | S/Places; isolated state, raw logs, traces and timings JSON | C; serial native runs; cold means an empty application cache, with OS/GPU caches untouched |
| `tools/bench/visual_check.py` | Two-build capture and connected-difference audit | S/Places; captures/report | C serial capture; A spawn comparisons |
| `tools/entities/build_beach_common.py` | Shared atlas derivative, rigid rig and export-sweep helpers | S/P/E; in-memory data and checks | D bounded builders |
| `tools/entities/build_beach_seagull.py`, `build_beach_crab.py`, `build_beach_fish.py` | Beach rig authoring / deterministic `--check` | S/P/E; GLB/source PNGs, temporary check output | D bounded direct construction |
| `tools/entities/build_mannequin.py` | Mannequin mesh and three poses; entity authoring | S/P/E; GLB and concrete PNG | D; bounded direct construction |
| `tools/entities/author_halloween_assets.py` | Material contracts for the Halloween entity GLBs; `--check` acceptance | S/E; GLB bytes and stdout | D; bounded single-file rewriting |
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
| `target/release/places --repair-geometry` | Read-only wall-joint repair planner; one level; `--plan` writes the machine-readable plan (same classification as `--check-geometry`) | S; one level | serial |
| `tools/levels/repair_alignment.py` | Verifies/applies a wall-joint repair plan byte-faithfully with an atomic replace, checker post-validation and idempotence proof (`--check` / `--apply`) | S; one level | serial |
| `tools/props/animate_spooner_man.py` | Canonical cat clip baking and --check | S/P; guarded canonical GLB | A; six spawn clip jobs, parent export |
| `tools/props/build_office_textures.py` | Native derivatives from Office masters / byte `--check` | S/P; native PNGs | D bounded derivative set |
| `tools/props/build_winter_textures.py` | Native derivatives from Winter masters / `--check` | S/T/I; native PNGs | D bounded derivative set |
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
| `tools/textures/author_home.py`, `author_pool.py`, `author_outdoors.py` | Explicit source/native artwork exports | S/T/I; committed PNGs | D kit authoring |
| `tools/textures/beach_art.py`, `frutiger_aero_art.py` | Committed-art readers plus explicit `--author` / byte `--check` | S/T/I for author/check; PNGs | D bounded authoring |
| `tools/textures/normalize_sky_seam.py` | Longitude-edge-only normalization / `--check` | S/T; named PNGs | D per-image |
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

`places-compile` runs one bounded CPU budget: `--workers N` selects it,
`PLACES_TOOL_WORKERS` is the environment fallback and the flag wins; `--workers
1` takes the serial atlas path. Its probe capture uses the one native GPU
sequentially. Builds publish atomically, so an interrupted run leaves the last
valid package in place.

The inventory highlights maintained entry points and their execution contracts;
theme-specific prop modules also live under `tools/props/parts/`. For arbitrary
authoring directories, `build-collection` writes sibling `.placesmap` files;
`tools/build-maps.sh` explicitly publishes the maintained lineup under
`assets/levels/`.
`tools/verify.sh` consumes the Python validation and native suites. The tracked
[GitHub Rust workflow](../.github/workflows/rust.yml) runs its own CI gates; see
[verification](VERIFICATION.md) for the required desktop checks. Historical functional
results, equivalence measurements and limitations are recorded in the
[feature-expansion summary](reports/feature-expansion-summary.md). These results describe
specific runs, not current gate status.

Native startup measurements use an already-built binary and a fresh output directory:

```sh
python3 tools/bench/loading.py --binary target/release/places --out target/loading-check --repeat 2
```

The tool runs each level sequentially with isolated application state, first
with an empty application cache and then reusing it. It does not flush OS or
GPU caches. Preserve the binary identity, logs and timing JSON when comparing
runs; do not run competing builds or CPU-heavy tooling during measurement.
