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
# Active content: explicit edit -> compile -> launch, separate from source CI.
cargo build --release
./target/release/places-compile build levels/office_demo.json \
    --out assets/levels/office_demo.placesmap --asset-root assets --workers 8
./target/release/places-compile validate assets/levels/office_demo.placesmap
./target/release/places-compile verify levels/office_demo.json \
    --package assets/levels/office_demo.placesmap --require-current
sh tools/build-maps.sh  # optional full maintained lineup; performs offline bakes
PLACES_LEVEL=office_demo ./target/release/places

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
# Historical reconstruction uses explicit scratch output, never archive writes.
python3 tools/levels/build_model_zoo.py --out /tmp/model-zoo-working.json --no-cache --workers 4
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
| Catalog `authoring: {"tool": "blender", "source": "<asset-relative>.blend"}` | Editable Blender-owned meshes. The source and GLB export must exist beneath `assets/`; ordinary and forced primitive builds preserve the export. Re-export through the source's documented Blender command, then run the same catalog/GLB/native checks. |
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
(`src/lighting/transport.rs`) for the current `full` variant: direct
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

Current packages contain Static (`off`) and Full lighting. Full prepares two
bounces with three emitter taps per axis at 16 texels/m; legacy Medium records
remain readable. Texture quality and filtering are independent of selected
lighting. `--workers N` selects the shared CPU budget, with
`PLACES_TOOL_WORKERS` as fallback and a maximum of 12. One worker is the serial
reference. See [verification](VERIFICATION.md) and the [package format](PACKAGE_FORMAT.md)
for current representation, atlas and allocation limits.

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

## Choosing a tool

This inventory follows imports, registries, CLI entry points and the build/CI
hooks. A tool's age or original author does not determine whether it is useful.
The [benchmark guide](../tools/bench/README.md) describes native and performance
campaigns; each CLI's `--help` names its inputs. Normal source tests never require
production map bakes.

| Workflow | Entry points and ownership |
| --- | --- |
| Fast source acceptance | `tools/assets/validate.py`, texture/prop `build.py --check`, maintained map generators `--check`, Python test discovery; exact automatic checks are in `.github/workflows/rust.yml` |
| Map publication | `tools/levels/compile.py`, `tools/build-maps.sh`; explicit eight-source allowlist, generated packages under ignored `assets/levels/` |
| Format conversion / wall repair | `convert_v3.py` retains tested v2 conversion; `repair_alignment.py` checks source hashes/old values and only applies with post-validation and rollback |
| Generated scenes | Maintained map owners in the [map index](../levels/README.md); `build_fixture_levels.py`, `build_outdoor_fixture.py`, `build_lighting_quality.py`, `build_capacity_fixtures.py` own focused controls |
| Historical source reconstruction | `build_model_zoo.py`, `build_outdoor_route.py`, `refine_home.py`, `refine_pool.py`; explicit working inputs/outputs, never the frozen archive as output |
| PNG / GLB authoring | `tools/textures/` saved-art readers and explicit exporters; `tools/props/build.py` dispatches registered `parts/` constructors; `mesh`, `tex`, `glb`, `geometry`, `glyphs` and `refreshed` are shared libraries |
| Distinct source owners | Office/Winter texture derivatives, Arcade/birthday/restroom/Bepsi artwork and models, Beach animals, original entity constructors and Spoonerman refinement/animation; use the owner table above |
| Asset inspection / repair | `audit.py`, `preview.py`, `validate_domestic_remake.py`, `repair_geometry.py`, `resize_embedded_textures.py`, seam tools, entity validators/contact sheets and `inspect_arcade_vending.py`; reports or explicit selected writes, no default map bake |
| Compiler measurement | `compiler_bench.py`, `compiler_compare.py`, `compiler_incremental.py`, `compare_packages.py`, `capture_compiler.py`; clean, paired and incremental experiments protect different claims |
| Native acceptance | `native_regression_maps.py`, `capture_art_style_hero.py`, themed camera scripts, movement/weather/traversal controls, `check_quality_regression.py`; serial real-player runs |
| Diagnostics / comparison | Loading traces, scene budgets, lightmap/probe/Metal inspectors, colour controls, Winter string-light audit, image comparisons and map storage/delivery tools; retain distinct numerical methods |
| Durable replay | `snapshot_art_style_hero.py` preserves an explicit binary/package and their dependencies; its default cameras now name the compact architecture fixture |

### Historical tooling and cleanup decisions

The Home, Pool and Outdoors capture commands keep their original frozen Demo
cameras, options, settings, failure checks and receipt format. Their identical
runner now lives in `tools/bench/capture_demo_views.py`; Outdoors still appends its
manifest while Home/Pool begin a new one. A matching preserved Demo payload is
required. These cameras are not valid substitutes for today's standalone maps.

Home/Pool concept dressing now requires `--source PATH --out PATH`. This removes
the broken implicit `levels/places_demo.json` lookup while retaining the authored
transformation. Archive output is refused. For example, reconstruct in scratch:

```sh
python3 tools/levels/refine_home.py --source docs/archive/levels/places_demo.json \
    --out /tmp/home-demo-working.json
```

`measure_capacity.sh` defaults to the maintained capacity fixture. Optional Demo
and Zoo measurements require `PLACES_CAPACITY_DEMO_PACKAGE` and
`PLACES_CAPACITY_ZOO_PACKAGE`; building the active lineup cannot provide them.
No archived source is reactivated or baked by default.

The four winding/cavity/UV-colour seam regressions formerly isolated in
`tools/props/test_geometry.py` are consolidated into `tests/test_asset_audit.py`
so normal discovery runs them. They use a triangle, quad or tetrahedron and export
no asset. Source/master generators, useful baseline controls and specialized
validators are retained. Superseded primitive constructors are also retained as
source provenance; current `PROPS` bindings select their refined owners. No
asset output, catalog binding or generator algorithm changed.

Documentation consolidates the eight superseded Winter pass reports into
[Winter development](reports/winter-final-audit.md). The developer and benchmark
guides now distinguish active workflows from historical replay; redundant machine
cleanup inventories and stale example launches are removed. Curated images,
immutable concepts, all eight active maps and both frozen historical JSON sources
remain unchanged. Unique untracked HDR/size audit evidence remains untouched.

The October 13 cleanup validation passed the 354-test Python source suite
(0 failures, 4 intentional opt-in skips), strict configured Clippy and workspace
Rust tests (2,269 passed, 18 existing ignored diagnostics). The focused native
empty-install smoke passed with the rebuilt release player, and its menu capture
was inspected. Direct/module capture CLI checks, all-camera/all-quality runner
comparison, scratch dressing repeatability/archive-write rejection and bounded
capacity orchestration passed. Catalog and prop checks passed; texture checks
passed with 96 existing size/manifest advisories, and Office source currency
passed. Tracked Markdown destinations/anchors and Python syntax were checked.

No artwork exporter, production map bake or broad native campaign ran. The
other seven installed packages remain unchanged/stale; archived Demo/Zoo were
not reactivated. Source CI and the publication handoff record exact revision
status separately. Unknown-owner untracked HDR/size audit files remain present.
