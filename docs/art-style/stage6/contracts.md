# Stage 6 contracts — 2026-10-08 UTC

[Native milestone](README.md) · [Costs and budgets](performance.md) · [Final milestone](README.md)

## Reusable hero content

The refined hero is an additive authoring source, not a renderer exception:
`tests/fixtures/levels/art_style_hero_refined.json`. It retains the Stage 5
transparency control's sky, light placement, water, ice backing, snow patch,
collision walls, timers and ghost spawn. The original and alternative authored
night/aurora/storm sources remain intact.

The chair has a new reusable catalog identity, `home:dining_chair_refined`, so
existing prepared maps keep the original chair's exact GLB and generator. The
refined model uses the same real, opaque 256² atlas and exact local footprint.
Bevelled stock, a crowned back and tapered feet improve its silhouette with
324 triangles; no texture master, aspect, UV quadrant, orientation or alpha
contract changes. Geometric flat normals follow the supported importer path;
model normal maps and authored tangents remain unsupported.

The room uses oak around a fitted rug, a throw cushion, two plants, wall art,
passage casing and five baseboard runs. Decorative models/trim are non-solid.
The six corrected furniture boxes use existing authored local `size` and
rotation semantics. No physics representation, movement rule or hero traversal
geometry changes. Plant alpha scenery remains excluded from opaque bake
occlusion by the existing placement flag. The stock casing's raised feet are
a reuse compromise reviewed in the native view, not new collision geometry.

The catalogue registration also requires one generated Model Zoo display.
Its existing Home apron receives that entry; all 217 old placements and every
other source field remain exact. This is the minimal registration contract,
not nonhero baking or visual adoption. The old package remains intact and its
intentional source currency gap belongs to Stage 7.

## Incremental compiler identity

A normal `places-compile build` recomputes the resolved source dependency closure:
placed props plus **every** spawn template, including unused override choices;
surface, fixture, decal and related resolved images remain covered. GLBs embed
model images under the existing importer contract, so their SHA covers runtime
geometry/material/image bytes. Changing a source PNG used to author a GLB first
requires regenerating that GLB; the adjacent authoring image is not separately
loaded by the player.

Package and prepared-stage keys include catalogue bytes, source inputs,
dependencies, installed compiler executable, capture mode, quality variants,
record/geometry/solver/light-model revisions and supported bake settings. The
executable hash uses an 8 KiB streaming buffer and is cached once per process.
Its measured startup cost is reported separately; it is not free.

The optional declared `build-inputs.json` record explains source/catalogue/tool
and capture identity without changing the package major or required capability
set. Old packages without compiler provenance rebuild once. Invalid provenance
or any corrupt declared product prevents reuse. `--force` bypasses both caches.
CLI and JSON `cache_decisions` explain package/prepared hits, misses and bypasses,
including individual added/removed/changed dependencies.

Unchanged valid packages are retained exactly. Metadata, supported navigation/AI
and final `environment.presentation` edits may reuse solved products. These
refresh semantics, navigation, provenance and atlas metadata to agree with an
independent full build. Exposure/shoulder/grade happen after HDR probe capture;
fog remains a physical/reflection input. Texture, material, model, geometry,
light and runtime entity-definition changes conservatively rebuild all affected
prepared variants. No unsafe per-file independence is assumed.

## Runtime currency and cache ownership

The player streams each declared external dependency's SHA during package-open;
same-size PNG/GLB substitutions now produce a named rebuild error. New packages'
optional provenance also guards the installed catalogue. Older packages lacking
that record retain the compatible immutable-bundle contract. A truly asset-less
embedded fallback retains its existing behavior. No frame or receiver hashes
files, and the player never compiles a map or solves static light.

Runtime `loading::PackageKey` uses canonical prepared-manifest identity and
quality. The old Stage 3 hypothetical live hot-texture atlas consumer no longer
exists: `loading.rs` installs decoded prepared variants. The only production
renderer fallback calls neutral preparation with lightmaps Off/cache None.
The compiler's transient `LightmapCache` starts fresh per build. Its public
source/settings helper remains suitable for immutable resolved inputs; manual
callers must clear a retained transient cache after editing those inputs. It is
not the owning incremental compiler cache. No second metadata/cache framework
is introduced to support a nonexistent runtime consumer.

Source and assets must remain stable during a compiler invocation. Queue
ownership serializes edits/bakes. Currency and reuse validate changed persistent
inputs between runs; no generic concurrent edit-and-restore snapshot facility
is claimed.

## Lighting and diagnostics

Moving casters still conservatively invalidate all admitted receivers. The
profiled optimization reuses the probe sample already computed for each support
anchor, rather than issuing the identical query twice. It preserves donor order,
connected fallback, visibility, global revision, static atlases, solver 15's
zero-source contract and skinned bounds proxies. The remaining 32-receiver cost
is reported, not disguised by the Stage 4 whole-engine timings.

New `visible_indices` counters accumulate actual successful indexed draws across
world/props/dynamics/characters/decals/effects. Triangle-list count is indices ÷3;
vertices remain vertices. Base-scene counters exclude sky, reflections, emission
duplicates, post and UI; they include depth-occluded submitted geometry. They do
not measure fragment overdraw or gameplay FPS. CSV adds one trailing field.
`scene_budget.py` checks package entry hashes and native manifest/quality identity,
rejects zero-draw telemetry, and emits configurable warnings. No renderer, parser,
asset or package safety bound is raised or weakened.
