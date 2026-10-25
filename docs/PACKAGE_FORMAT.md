# Compiled map packages (`.placesmap`)

A `.placesmap` file is the only thing a player loads. It is a ZIP archive
containing one level or a complete authored project. Each level retains its
validated gameplay semantics and the prepared static world the offline compiler produced from an authoring source. The player does
not compile maps, bake light, pack charts, capture reflections or generate
static navigation; it decodes these records and uploads them.

The repository tracks authored maps in `levels/`, with the maintained IDs
listed in `levels/lineup.txt`. Compiled `.placesmap` files are untracked build
outputs. Rust builds and source tests work before any map is compiled; the
player needs compiled packages to play.

From a fresh checkout, build the release tools and the complete maintained
lineup explicitly:

```sh
cargo build --locked --release
python3 tools/levels/compile.py --workers 8
./target/release/places
```

`sh tools/build-maps.sh` builds the release compiler and runs the same lineup
tool. It compiles Static (`off`) and Full variants, with reflection capture for Full, into
`assets/levels/`, verifies each package against current inputs, and validates
every record. After editing one source, build its output explicitly:

```sh
./target/release/places-compile build levels/home_showcase.json \
    --out assets/levels/home_showcase.placesmap --asset-root assets
```

The player discovers bundled packages in `assets/levels/` and installed packages
in its platform user-data `levels/` directory. Use `places --paths` to inspect
resolved resource/data/import locations; see [installation and overrides](INSTALLATION.md).
Authored JSON never becomes a playable row.
There is no compiled Demo inside the executable: an absent Demo reports the
explicit build/import action, and installing a valid package restores loading.
Bundled maps are optional distribution content. A package can ship the player and
runtime assets with no bundled maps; installed maps remain discoverable.
When a distribution includes maps, verify the selected packages against current
inputs before exporting them. Packaging never compiles maps on the player's behalf.

`places-compile` reuses an existing package when the source, the referenced
asset identities and the compiler version are unchanged; `--force` rebuilds.
A failed build leaves the previous package untouched.

## Transport and exact uniform tiles (0.13.0 compiler pass)

New binary ZIP entries use Zstandard method 93 at the selected bounded level
(ordinary publication 6, balanced storage migration 9, explicit compact migration 19).
Top-level bootstrap JSON remains Deflate at level 9 or below.
The shared compiler, player, import and editor reader opens method 93 through
its own raw-entry decoder: maximum history **8 MiB**, conservative live codec
reservation **16 MiB**, **64 KiB** input buffering, and expansion ceiling
`compressed_bytes * 65,536 + 64 KiB`. Existing per-record and aggregate output
limits still apply. Exact declared length, ZIP CRC and content-addressed SHA-256
are checked. Truncation, excess output, excessive windows, concatenated frames
and trailing bytes are rejected; a decoded-byte limit alone is insufficient.
Zstandard is linked from the supported Rust crate and needs no runtime program.

Explicit Python payload-inspection tools share `tools/levels/package_zip.py`.
For method 93 it invokes this workspace's bounded `package-repack` tool once
per opened archive, reads an exact-record temporary Deflate copy, and removes
that copy at close. Original ZIP metadata remains the source of byte accounting;
this compatibility repack is excluded from codec timing. Build the workspace
tools, or set `PLACES_PACKAGE_REPACK` to a preserved matching `package-repack`
executable after Cargo cleanup. Python tools that only read top-level bootstrap
JSON keep their existing readers. These developer tools are independent of the
game/editor/compiler runtime codec.

Ordinary map format 1 requires `zip-zstd-v1`. New project indexes use project
package transport revision **2** so old readers refuse before accessing new
compressed records. The current reader also accepts revision 1 project indexes
and existing Stored/Deflate records. Editable source `project_format`, application
version and ordinary map major version are unchanged. `optimize-storage` writes
a separate destination; it never replaces the valid source package.

PLC1 revision **2** adds mode 3: one exact six-channel binary16 value for an
entire uniform 16-square tile (including short edge tiles), with reserved alpha
0.5 restored as before. The mode occupies 16 bytes including its block header.
Modes 1/2 and revision 1 remain readable. Uniform detection preserves revision
1's decoded half bits and fused-add signed-zero behavior. Nonuniform ranges,
packed coefficient bits, exact fallback, quality, charts, coordinates, samples
and bounce settings are unchanged. New compact maps require
`lightmaps-compact-v2`; unsupported readers refuse rather than dropping detail.
Migration validates and compacts the owned record in place, retaining every
nonuniform tile's bytes and never solving or re-quantizing lighting. This reduces
encoded record/output overlap; the expanded runtime atlas allocation is unchanged.

## 1. Archive layout

```text
manifest.json                     small JSON index: version, identity, entries, dependencies
build-inputs.json                 optional compiler provenance (declared compiler-inputs entry)
semantics.json                    the validated, authoring-prepared LevelDef
blobs/<sha256>.mesh               static geometry (binary record)
blobs/<sha256>.props              transformed static prop batches (binary record)
blobs/<sha256>.lighting           baked LevelLighting (binary record)
blobs/<sha256>.collision          static collision primitives (binary record)
blobs/<sha256>.navigation         baked cell navigation mesh (binary record)
blobs/<sha256>.lightmaps.pllc     compact HDR/moment atlas (PLC1; expands to RGBA16F pairs)
blobs/<sha256>.lightmaps.ktx2     explicit exact HDR atlas (KTX2 RGBA16F pairs)
blobs/<sha256>.props.recipe       exact prepared-prop reconstruction recipe (PPS1)
blobs/<sha256>.props.shared       shared prepared-model byte segments (at most 16 MiB)
blobs/<sha256>.lightmaps.json     atlas chart/stat/config record
blobs/<sha256>.probe.ktx2         one reflection probe cube (KTX2 cube, RGBA16F, roughness mips)
blobs/<sha256>.probes.json        probe positions and mip level count for one face size
blobs/<sha256>.irradiance         prepared irradiance field for moving objects (binary)
blobs/<sha256>.png                embedded texture pixels (community packages)
```

Every blob name embeds the SHA-256 of the entry's uncompressed bytes. Readers
recompute the digest before decoding and refuse a mismatch. Fresh ordinary builds write entries
in sorted order with a fixed timestamp and compression profile. Incremental
project exports may retain verified existing compressed streams, including legacy
methods, so equivalent decoded content need not have identical archive bytes or
size. Content-addressed identities continue to hash decoded record bytes.

An ordinary map’s `semantics.json` is its prepared gameplay JSON. It is a
`LevelDef` (current authored schema: `schema: "places-level"`, `format_version: 1`, typed components,
event bindings, actions, conditions, sequences, timers and spawn records)
after `prepare_level` (fixture alignment, decal snapping, generated baseboards);
the player validates it again but never emits geometry from it. Runtime JSON
never triggers procedural geometry generation.

## 2. Manifest

```json
{
  "schema": "places-map",
  "package_format": 1,
  "id": "office_demo",
  "name": "Office: After Hours",
  "author": "Places",
  "created_by": "places-compile 0.9.0",
  "compiler_fingerprint": "<sha256>",
  "required_capabilities": ["geometry", "props", "lighting", "collision",
                            "lightmaps-hdr", "irradiance-probes", "probes-hdr"],
  "dependencies": [
    { "kind": "model", "path": "shared/props/table.glb",
      "sha256": "<sha256>", "bytes": 123816 }
  ],
  "entries": [
    { "name": "blobs/<sha256>.mesh", "role": "mesh",
      "bytes": 1217874, "sha256": "<sha256>" }
  ],
  "variants": [
    { "lightmap_quality": "full", "quality_profile": "full",
      "lightmap_failure": null,
      "entries": {
        "mesh": "blobs/...mesh", "props": "blobs/...props",
        "lighting": "blobs/...lighting", "collision": "blobs/...collision",
        "lightmaps": "blobs/...lightmaps.ktx2",
        "lightmaps_meta": "blobs/...lightmaps.json",
        "irradiance": "blobs/...irradiance",
        "probes": [
          { "face_edge": 48, "count": 2, "levels": 6,
            "cubemaps": ["blobs/...probe.ktx2", "blobs/...probe.ktx2"],
            "positions": "blobs/...probes.json" }
        ]
      } }
  ]
}
```

* `required_capabilities` names what a runtime must understand. The current
  player understands `geometry`, `props`, `lighting`, `lightmaps-hdr`,
  `lightmaps-compact-v1`, `props-shared-streams`, `props-shared-streams-v2`, `props-shared-streams-v3`, `props-shared-streams-v4`,
  `irradiance-probes`,
  `probes-hdr`, `collision` and
  `navigation`, `light-circuits-v1`, `static-light-state-v1`, `luau-v1`,
  `scene-actions-v1`, `world-observations-v1`, `project-transitions-v1`, `full-lighting-v1` and
  `item-receipts-v1`. A package
  requiring anything else is rejected by name.
* Semantic `requires_full_lighting: true` requires `full-lighting-v1` and a
  prepared `full` atlas with its metadata and no recorded fill failure. Compiler
  publication and player admission refuse a vertex fallback for this policy.
  The player refuses a different selected lighting
  quality with a named Full-lighting instruction. Omitted/false preserves the
  existing quality choices. This policy changes no prepared record format and
  is excluded from the prepared-stage fingerprint.
* Volumes using `item_receipt: "centre"` or `"settled_centre"` require
  `item-receipts-v1`; admission refuses an omitted capability so an older
  host cannot silently use overlap receipts. Default overlap adds no capability.
* An Off variant may carry `static_light_off: { "fixtures": [0, 1],
  "mesh": "blobs/...mesh", "props": "blobs/...props" }`. It requires
  `static-light-state-v1`; fixture indices are unique, ascending and bounded to
  one through eight, then checked against decoded lighting. Mesh/props use the
  existing bounded codecs and shared prop streams; images remain shared. Full
  variants cannot carry this record. Navigation-only reuse preserves its refs
  and all referenced dictionary blobs. Maps without this optional state omit it.
* `dependencies` records the content identity of every model and texture a
  package reads from the installed asset bundle (`kind: "model"`/`"texture"`,
  root-relative path) or embeds itself (`kind: "embedded"`). A package whose
  declared dependency is missing or has a different size or SHA-256 is refused
  at load with an error naming it. Opening an archive without an asset root can
  validate its internal records and logs that installed dependencies cannot be
  checked; it does not establish that external models or textures are available.
  Embedded community texture entries remain part of the archive format.
  `places-compile validate` re-hashes every dependency and reports any content
  difference. Models include every placed prop and every runtime spawn template,
  including unused templates selectable by an override. GLBs embed their PNG
  images under the existing importer contract, so the model hash covers that
  runtime image closure. The player streams installed dependency hashes during
  package-open, with an 8 KiB buffer; rendering does not hash them per frame.
  The compiler uses the same bounded streaming reader. Canonical dependency
  paths must remain inside the selected asset root, regular files are required,
  and the actual GLB/PNG encoded limits (64/32 MiB) apply even if a package
  declares a larger size or an installed file grows while being read.
  Duplicate `(kind, case-folded path)` dependencies are rejected before hashing.
* `entries` must list every archive entry except `manifest.json` itself.
  Undeclared entries, duplicate declarations and declared-but-missing entries
  are errors.
* `compiler_fingerprint` is a **developer rebuild identity**: exact selected
  source, the map-local catalog closure, resource hashes, record versions,
  requested variants, geometry/light-model/solver revisions, compiler preparation
  source identity and reflection-capture mode. `build.rs` hashes preparation
  sources, shaders, embedded preparation assets, dependency/toolchain inputs,
  target and features; it never compiles a map. Game/editor application code,
  compiler UI/CLI, executable packaging and debug symbols are excluded. Engine
  source changes conservatively invalidate every prepared stage. Paths and file
  timestamps never enter this key.
* `build-inputs.json` is an optional declared `compiler-inputs` entry. Revision 2
  carries `source_sha256`, informational `catalog_sha256`/`tool_sha256`,
  `compiler_code_sha256`, `capture_probes` and `catalog_inputs`. The closure is
  itself revisioned: sorted logical IDs map to exact catalog records or explicit
  absent lookups, including recursive material/image/baseboard references,
  props/templates, fixtures, decals and sky. The player recomputes this closure
  against installed assets; unrelated definitions can change, while referenced
  definitions and file hashes must still match. Revision 1 remains readable
  under its original whole-catalog runtime check, but requires one compiler
  rebuild to acquire revision 2 reuse keys. Unsupported or corrupt provenance
  never becomes a cache hit. Storage migration retains original provenance.
  This entry remains optional for data-only packages from other producers.
* `lighting_fingerprint` identifies prepared geometry, collision, lighting and
  reflections. It retains source materials/images/models, physical environment
  and fog, lights, entities, tool/capture identity and formats. The v2 stage
  key excludes requested quality selection/order; each existing quality can be
  reused independently while missing qualities are prepared. Preparation source
  identity covers all quality profiles, so code/profile edits invalidate all.
  Cache decisions expose `prepared_world/<quality>` hits/misses/bypasses and an
  aggregate `partial` result when some requested qualities are reused.
  Older stage keys miss conservatively once. Full package identity retains
  requested quality selection/order and complete current provenance. It
  excludes display name/author, navigation/AI components and
  Luau modules, the required-lighting policy and `environment.presentation`
  (exposure, shoulder and grade after HDR capture).
  A validated match reuses solved records and refreshes semantics, navigation,
  provenance and atlas metadata; prepared texels and probes remain unchanged.
  Every declared record is hash-verified before reuse. `--force` bypasses both
  package and prepared caches. CLI output and JSON `cache_decisions` name each
  hit, miss or bypass and its input/integrity reasons. Reuse reports may omit
  preparation counts; inspect actual records rather than treating zeros as
  removed content.

Compiler builds require stable source and asset inputs while running. The queue
serializes authoring and builds; persistent edits between runs invalidate the
appropriate key. No concurrent edit-and-restore snapshot system is introduced.
The player loads prepared packages, never a live transport bake. The public
transient atlas helper's source/settings key requires immutable resolved inputs
or an explicit cache clear; it is not the asset/map incremental cache. See the
[Stage 6 contracts and audit](art-style/stage6/contracts.md).

## 3. Quality variants

Standard builds carry Static (`off`) and Full (`full`); the `medium` variant is
also supported by the current codec. Each variant is a complete
prepared world for that setting:

* `off` is the historical vertex-lit build: prepared geometry with baked vertex
  colours and no atlas. It is a supported, intentional variant, not an empty
  package.
* `full` carries prepared geometry and the full-quality atlas; `medium` carries
  its corresponding prepared atlas.
* A variant whose atlas could not be filled records `lightmap_failure` and
  ships the vertex-lit geometry instead, exactly as the runtime used to fall
  back — the fallback is recorded at compile time, never re-derived at load.

`quality_profile` records the bake/plan profile (`low` for `off`, `full`
otherwise) so a future reader can tell which bake produced the vertex colours.

A decoded package whose prepared atlas the device cannot bind (more than fourteen
pages, or more than four switchable groups: a combination the compiler can
never emit) falls back to a vertex-lit rebuild of the decoded semantics. That
is the loader's defensive recovery path, not a player-side bake of the prepared
path; no compiler output reaches it, and the prepared solve never runs in the
player.

Full's profile planning cap is fourteen pages; lower profiles retain eight.
Architecture uses MAXRECTS until model receivers begin. Its free-rectangle union
is then partitioned into disjoint guillotine holes, retaining all existing chart
coordinates and occupied texels. Model charts reuse this free area immediately.
Exact adjacent disjoint holes are joined before opening another page; physical
sampling density, inclusive endpoints and gutters remain unchanged. Atlas content
key revision 1 identifies the current layout. Superseded packages require recompilation.
Before geometry emission, the planner also bounds that cap by the actual base
and switchable illumination groups, the typed record bytes (including the exact
196-byte single-mip KTX2 header), the codec payload/layer bounds and the decoder
page count. At 1024px, Full allows 14/12/8/6/4 pages for zero through four
switchable lights. The completed Arcade catalog can retain eight pages for its three groups. The
effective config is shared by the plan, fill and public cache-key refresh.
The model-lighting correction shares compatible coplanar source-triangle charts
and gives small opaque props 32 intervals/m (Medium24/m). Its measured Hallows
Full layout needs eleven 1024px pages, or 176 MiB for the base group, preserving
the finer physical shadow field. The earlier
[Stage 7 Hallows audit](art-style/stage7/integration-contract-audit.md)
established ten pages at its former sampling density. The independent typed
atlas bound is384 MiB+64KiB, admitting the approved denser Arcade receiver
set without lowering sampling density. The previous320MiB bound admitted the
ten-page two-group Demo payload but capped Arcade at six pages and caused a
whole-world vertex fallback. The
[record audit](art-style/stage7/integration-contract-audit.md) records
the old 256 MiB rejection and 25% increase. The 64-page decoder count, exact
KTX2 shape/length checks, four-switch-group limit and 1 GiB aggregate remain
enforced. More groups still have to fit the typed byte bound; fourteen pages do not
authorize an arbitrary total allocation. The aggregate guard still applies to
the complete archive, including all quality variants.

## 4. Binary records

All integers are little-endian, records are byte-aligned and every record ends
exactly at its declared end (trailing bytes are an error). Counts are bounded
before allocation; floats must be finite; indices must resolve inside their own
record.

### 4.1 Geometry — `blobs/<sha>.mesh`

```text
magic "PMW1" | version u16 = 1
range_count u32
batches 6 x i32    floor, ceiling, wall, light, prop-fallback, decal (indices)
vertex_count u32   declared total
index_count u32    declared total
ranges range_count x {
  kind u8          0 floor, 1 ceiling, 2 wall, 3 light, 4 prop fallback, 5 decal
  material u32     level material index; 0xFFFFFFFF = no level material
  shine u8         0 = none; 1..=101 = whole-percent override (value - 1)
  bounds_min f32 x 3
  bounds_max f32 x 3
  vertices u32 count, then 81-byte vertices
  indices u32 count, then u16 indices
}
vertex (81 bytes):
  pos f32 x 3 | color f32 x 4 | uv f32 x 2 | normal f32 x 3 | tangent f32 x 3
  | handedness f32 | lightmap u16 x 2 | lightmap_page u8 | caustics_albedo f32 x 3
```

A non-sentinel material above the level budget (131,072) is refused.
`caustics_albedo` is the linear material/vertex factor captured before baked
lighting. It keeps source-coloured focused water light independent of ordinary
vertex-colour lighting. Model pigment, prop tint, surface tint and face shade
remain part of this factor. Every current vertex carries it explicitly.

`lightmap_page = 0xFF` marks a vertex that takes the vertex-colour path.
`normal = (0,0,0)` is the smooth-normal sentinel; the compiler resolves it
before writing, so a decoded record never contains one. The record carries the
final vertices, indices and draw ranges; nothing is regenerated at load.

### 4.2 Static props — `blobs/<sha>.props`

Prop vertices are pre-transformed and pre-lit. Model textures are **not**
embedded: the record names the model and each submesh's texture slot, and the
player attaches the decoded images from the same catalog model the manifest
identifies by hash. One copy of model artwork stays on disk.

```text
magic "PMP1" | version u16 = 1 | batch_count u32
batch {
  model u32 length + UTF-8
  casts_static_lighting u8 (strict 0 or 1)
  light_control optional u32 (strict u8 presence, then controller when present)
  bounds_min f32 x 3 | bounds_max f32 x 3
  submesh_count u32
  submeshes { texture optional u16 | alpha_mode u8 (0 opaque, 1 cutout, 2 blend)
              | alpha_cutoff f32
              | emission colour f32 x 3, intensity f32, mask optional u16
              | specular f32 x 3 (linear), roughness f32
              | uv_response f32, uv_mask optional u16, uv_tint f32 x 3 (linear)
              | first_index u32 | index_count u32 }
  vertex_count u32 | vertex_mode u8
  if vertex_mode = 1 or 2: frame_count u32 + frame_count x 28-byte frame
  vertices vertex_count x vertex_storage
  indices u32 count + u16
}
frame (28 bytes): normal f32 x 3 | tangent f32 x 3 | handedness f32
vertex_storage:
  mode 0: literal 81-byte neutral vertex from section 4.1
  mode 1: pos/color/UV prefix (36 bytes) | lightmap/page/caustics tail (17 bytes) | frame u16
  mode 2: the same prefix and tail | frame u32
```

The optional light controller identifies independently controlled fixture models
and fixed props: fixture controllers use their authored ordinal, and prop
controllers use the fixture count plus the prop ordinal. Controlled instances
retain separate luminous uniforms while sharing source models, PNGs and ordinary
material allocations.

Every current vertex includes its 12-byte neutral `caustics_albedo` factor.
Exact vertex interning includes this factor and preserves complete vertex
identity, first-occurrence order, ordered triangle indices and unreferenced
vertices. Frame palettes preserve exact normal/tangent/handedness bytes,
including signed zero. A palette is used only when its count, header, frames
and references are strictly smaller than literals; ties retain literal storage.
Mode 1 permits at most 65,536 frames; mode 2 uses u32 references.

Frame counts cannot exceed vertex counts. The decoder bounds complete runs before
allocation and rejects unknown modes, invalid references, truncation and nonfinite
frames, including unused frames. Both writer and reader enforce a cumulative
512 MiB expanded literal-record budget: ten record-header bytes, batch metadata
through the vertex count, 81 bytes per vertex, four index-count bytes and two
bytes per index. Encoded records have their own 512 MiB limit.

Blended submeshes carry opacity 1.0: imported GLB `baseColorFactor` alpha is
already folded into vertices, while per-instance fade remains runtime state.
Submesh draw ranges and every index must resolve inside their batch.

Scalar material response is mandatory. The static-caster flag is 0 for fully
claimed character models and 1 for static or partial/overflow fallback models,
using the same bounded claim plan as character spawning. This prevents a moving
actor from leaving a fixed bind-pose caster or rejecting its own probe rays.

Ultraviolet response is finite in 0..=2, with an optional numeric-mask texture slot
and independent linear tint in 0..=1 per channel. Model images are reattached after
dependency verification. Runtime enabled fixtures and visibility evaluate this
response; it adds no incident irradiance or emission-only bloom energy.

### 4.3 Baked lighting — `blobs/<sha>.lighting`

The complete `LevelLighting` bake: rooms, baselines, powered zones, resolved
lights, opening blends, room-to-light candidate lists, fixture-to-light
mapping, the sampling-tap count and the built static visibility set (wall,
ceiling/floor and prop occluders plus each query site's reachable-solid pool).
The compiled lighting record starts with magic `PLT1` and u16 revision 1.
Each local source stores a strict caustics bool immediately after its enabled
flag. After the local-light list, a u8 global count (maximum eight) precedes each
active directional source: incoming XYZ and RGB f32 triples, f32 intensity,
bool8 shadow flag, f32 angular radius in radians, and bool8 caustics eligibility.
Three finite nonnegative sky-radiance floats follow. Unit directions, energy and
angles are validated. Per-water controls remain in `semantics.json`.
The current geometry, solver and atlas key revisions are all 1; old prepared
lighting must be recompiled rather than decoded through earlier layouts.
The player keeps this record resident because runtime entity lighting and
fixture switches read it:

* dynamic objects and characters refresh current entity lighting, with cached
  spatial payloads invalidated by transform, field, source or rigid
  movable-caster changes;
* switchable fixtures update the enabled mask of prepared layer groups and
  their luminous-face emission scale without rewriting atlas pages.

Rebuilding this record at play time would be the static lighting bake, so it
travels in the package. The index structures (room and light lookup grids) are
rebuilt deterministically from the decoded lists; they are accelerators, not
baked content.

### 4.4 Static collision — `blobs/<sha>.collision`

```text
magic "PCL1" | version u16 = 1
walls    u32 count + (min_x,min_y,min_z,max_x,max_y,max_z,step_up) f32 x 7
floor    u32 room count + rooms (footprint, floor_y, ramps, stairs, regions)
ceiling  u32 room count + rooms (footprint, floor_y, height, profile)
water    u32 volume count + volumes (footprint bounding box, surface_y, bottom_y,
                                    opacity, shape u8, radius f32, swimming,
                                    optional material id)
ladders  u32 ladder count + ladders (footprint, bottom_y, top_y, facing)
```

`shape` is `0` for the historical rectangle (the bounding box *is* the
footprint and `radius` is ignored) or `1` for a circle inscribed in the
bounding box (`radius` must equal half of both extents). Every current record
carries the shape explicitly, preserving the exact footprint the player queries.

Interactive semantics (doors, interactables, trigger volumes, entity routes,
lights, timers, sequences and spawns) stay derived from the validated semantic
record: they are O(authored entities) gameplay state, not static collision. The
player loads the record's typed component/binding/action data into the entity
runtime and instantiates only the dynamic state each object needs; the prebuilt
`CollisionIndex` grid is rebuilt from the decoded wall boxes at start.

### 4.5 Baked navigation — `blobs/<sha>.navigation`

```text
magic "PNV2" | version u16 = 2
cell_m, origin_x, origin_z      f32 x 3
cells_x, cells_z, node_count    u32 x 3
column_offsets                 u32 x (cells_x * cells_z + 1)
class_count u32; per class: radius, height, step_height, max_slope f32 x 4
portal_count u32; per portal: door id (u32 length + UTF-8, <= 256 bytes)
node_count surface nodes, column order then strictly ascending Y:
  y f32; flags u8 (bit 0 surface, bit 1 continuous slope)
  headroom_cm u16 (65535 = unbounded); portal u16 (65535 = none)
per class: walkable u32 byte length + ceil(node_count / 8) bytes
per class: region u32 count + u16 x node_count (65535 = none)
per class, per node: outgoing u32 x 8 (4294967295 = no traversal)
```

Each XZ column has zero or more floor spans; `column_offsets[c]..column_offsets[c+1]`
addresses its nodes. Stacked rooms retain separate heights, body clearance and
links. The outgoing directions are `(-1,0), (1,0), (0,-1), (0,1), (-1,-1),
(1,-1), (-1,1), (1,1)`. Each class is a distinct authored `nav_agent` body plus
the reference humanoid. Runtime class matching remains exact within 0.0001 m.

The compiler checks each body's disc, headroom, solid collision and swimming
exclusion against the same floor/ceiling and collision model as movement.
Headroom uses the ceiling above that floor and upper-room undersides. Stored
links prove traversal with movement-sized surface substeps and continuous disc
sweeps; a thin wall between cell centres cannot disappear from the route.
Diagonals require both orthogonal paths to the identical floor span. Links are
reciprocal; conservative drops use the body's step limit unless continuous
slope traversal is proven. Connected-region labels are derived from these links.

Door portals include height, so a door on one storey does not block another
storey at the same XZ. Live doors and moving-obstacle overlays affect queries
without rebuilding the graph. Record readers check offsets, ascending finite
heights, class masks/regions, bounded nodes and columns, per-body link envelopes,
reciprocal adjacent-column links and matching region labels. The 128 MiB record
budget includes `4*(columns+1) + 9*nodes + classes*(34*nodes + ceil(nodes/8))`
plus bounded framing/portal metadata; it is checked before class-run allocation.

The package container stays at version 1. Navigation alone deliberately changes
to PNV2/version 2; PNV1 is refused with a named recompilation requirement. The
compiler fingerprint includes this record version, so source rebuilds invalidate
old navigation. There is no runtime migration, geometry bake or repair path.

### 4.2.1 Exact shared prepared prop data

The compiler factors repeated current PMP1 streams losslessly when the dictionary
saves both raw bytes and publication-codec archive bytes, including a conservative
manifest allowance. Recipes exceeding the reader working-set budget remain
literal. The compiler compares the actual publication codec cost of PPS1 versions 2 and 4,
retaining the smaller valid representation. Both separate each vertex into exact 12-byte
world position, 16-byte baked colour, 8-byte model UV and remaining
lighting/frame planes. Changing baked colour therefore cannot prevent identical
position or UV bytes from sharing. The version 1 reader retains its combined
36-byte position/colour/UV prefix and separate tail. All four versions preserve all
currently accepted vertex strides (43, 45, 55, 57, 69 and 81 bytes). Raw headers,
material data, frame palettes and ordered indices remain exact. First occurrence
order interns chunks of at most 65,520 bytes into at most 16 MiB shared segments.
Version 4 transposes each attribute plane into byte lanes within
65,536-vertex tiles before chunk interning. This is an exact byte permutation,
including signed zero and every floating-point bit. The decoder uses one bounded
tile scratch buffer (at most 2,949,120 bytes, included in the 1 GiB budget) and the original output allocation;
versions 1 and 2 retain their original layouts; version 3 retains its 256-vertex shuffle tiles. Original GLBs and PNGs remain
external dependencies.

A PPS1 recipe replaces a variant's `props` entry (role `props`). It records the
original PMP1 length/hash, required segment lengths/hashes, bounded chunk
locations, and ordered raw or interleaved vertex runs. The reader verifies
segment identities and reconstructs one output directly through borrowed chunk
cursors; it verifies the original record hash before the current PMP1 reader
validates its geometry. No complete intermediate attribute arrays are built.
The 512 MiB expanded-record limit remains. Recipe wire data is capped at 64 MiB;
output, fetched segments and conservative recipe-table overhead together are
capped at 1 GiB. Missing, truncated, out-of-range, unused or trailing
recipe data is rejected. Version 1 packages require `props-shared-streams`;
version 2 additionally requires `props-shared-streams-v2`, version 3 also requires `props-shared-streams-v3`, and version 4 additionally
requires `props-shared-streams-v4`, so older players fail
by named capability before fetching these streams. Current literal PMP1 records
remain valid when sharing would not save space. This changes storage only: no
float quantization, triangle reduction, texture or lighting quality change, or
runtime geometry reconstruction from a different source model is introduced.

## 5. Lightmap pages — compact storage and KTX2

The default writer emits metadata revision 1 and `blobs/<sha>.lightmaps.pllc`.
PLC1 has a 24-byte little-endian header (magic, version, 16-pixel tile edge,
page edge, page count, contribution-group count and zero reserved word).
Each page pair is encoded in 16×16 tile order. A compact tile has a 4-byte mode
header, six independent `(minimum, step)` f32 ranges, and one 64-bit word per
texel: RGB11 and signed physical moment XYZ10 (63 bits; high bit zero).
Local ranges expand to linear coefficients before filtering. Nominal compact
storage is 8.203125 bytes/texel including tile framing at a full tile.

The encoder compares independent colour precision, signed moment energy and
reconstructed light, including an all-normal error bound. A failing tile stores
all six source half-floats exactly (12 bytes/texel plus framing). It never changes
exposure to meet precision. Both modes imply reserved alpha 0.5. Constants,
dark channels and HDR outliers retain the same bounded validation. Framing and
declared size are checked before the at-most-384-MiB RGBA16F allocation; ranges
and coefficients are checked during decode. Malformed
versions, ranges, coefficients, reserved bits, truncation and trailing bytes fail.

The runtime retains shared packed half-float page views and uploads their bytes
directly, avoiding KTX image copies and half→f32→half arrays. Scalar diagnostics
read a texel on demand; a full diagnostic slice explicitly materializes floats.
GPU format/storage remain two Rgba16Float planes (16 bytes/texel/group), with
one mip and clamped bilinear filtering. Disk compression does not reduce GPU
memory. The higher-precision `write_lightmaps_precise` writer and metadata
revision-1 KTX2 reader preserve all six half-float coefficients exactly.

`blobs/<sha>.lightmaps.json` holds the revision-1 chart record:

```json
{ "record_version": 1, "page_edge": 1024, "page_count": 2, "padding": 2,
  "content_key": "v1-…",
  "stats": { "charts": 1062, "pages": 2, "texels": …, "page_texels": …,
             "bake_millis": …, "cache_hit": false },
  "charts": [ { "patch": { "origin": […], "u_axis": […], "v_axis": […],
                           "room": 3, "kind": "floor" },
                "chart": { "page": 0, "x": 0, "y": 0,
                           "width": 512, "height": 384 } } ],
  "switchable_lights": [21] }
```

The pages are one uncompressed KTX 2.0 file: `VK_FORMAT_R16G16B16A16_SFLOAT`,
no supercompression, two 2D array layers per page (the page's irradiance plane
then its direction-moment plane), the default `rd` orientation, and the standard
four-sample RGBSDA half-float descriptor. RGBA16F is the portable linear HDR
reference the renderer uploads directly; the alpha channels of the two planes
are **reserved** (writers store `0.5`) and consumers ignore them. Every page of
the base solve is followed by one page set per entry in `switchable_lights`, in
order: the shader sums the reconstructed sets the live light mask selects, so a
runtime switch changes real illumination without a runtime bake.

Each texel stores the offline transport solve in linear HDR: an irradiance mean
`I` and the vector sum `g` of the per-channel first moments, reconstructed as
`max(0, I + (I / max(I.r + I.g + I.b, 1e-6)) * (2 * max(0, dot(g, n)) - |g|))`.
The form is exact for any number of contributions sharing one direction of any
colour (`2 * I * max(0, cos)`), evaluates its only nonlinear step on the
*scalar* `dot(g, n)` of the interpolated moment vector (so a hardware bilinear
interpolation preserves a continuous moment field),
collapses smoothly to the isotropic mean where the moment cancels, and is
bounded by `2 * I`. Surface albedo is never folded into these values; the
fragment shader multiplies the base colour exactly once.

The current writer emits exactly one mip level; the reader carries up to 16
levels so a future prefiltered payload can add them without a new container.
Reading is otherwise a strict subset: supercompression, block compression,
non-RGBA16F formats, key/value metadata, mixed array/cube payloads, a level
whose length disagrees with its dimensions, and trailing bytes after the last
level are all rejected by name before a large allocation.

## 6. Reflection probes

Probes are captured **offline** by `places-compile` during every `build`: the
compiler installs the prepared world in a window-free renderer and renders the
same six faces per probe the player used to render at load time. Each captured
cube is then **prefiltered offline into a roughness mip chain** (level 0 is the
capture; level `L` is a cone average whose roughness is `L / (levels - 1)`) and
packaged as one KTX2 cube with that full chain. Rebuilding the package
re-captures and re-prefilters them; the player never prefilteres at load and
never re-captures on a graphics change.

```json
{ "record_version": 1, "face_edge": 64, "levels": 7, "points": [[x, y, z], …] }
```

Positions revision 1 identifies linear HDR RGBA16F cubes. The writer declares
`probes-hdr` and retains authored HDR through capture and prefiltering before
runtime presentation. RGBA8 compatibility decoding is not supported.

Both runtime face sizes are packaged (48 texels for Reflections Medium, 64 for
Full), each with its own chain (`48 -> 6` levels, `64 -> 7`). The player
uploads the resident size when it installs the world; a Reflections-setting
change stages a new installation that uploads the other size and its chain.
The shader selects the mip from the material's roughness, so a polished floor
reads the capture almost sharp while a rough one reads a wide prefiltered
lobe. Plane mirrors stay frame-dependent: the mirrored view is still rendered
each frame the setting allows.

A package whose geometry routes probes but which has no captures is refused at
load: the player will not silently render the level without the reflections its
materials ask for.

### 6.1 Irradiance field for moving objects

`blobs/<sha>.irradiance` is the prepared probe field moving objects and
characters sample every frame instead of re-baking light:

```text
magic "PPF1" | version u16 = 1
origin f32 x 3 | cell_m f32 | dims u32 x 3 | count u32
probes count x {
  irradiance f32 x 3 | direction f32 x 3 | axis f32 x 2 | room i32
}
local_direct_present u8 (strict 0 or 1)
if local_direct_present = 1:
  selected_source_count u32 | selected_source_indices u32 x selected_source_count
  selected_local_direct count x { irradiance f32 x 3 | direction f32 x 3 }
switchable_group_count u32
switchable_indirect switchable_group_count x {
  representative_light_index u32
  coefficients count x { irradiance f32 x 3 | direction f32 x 3 }
}
```

The field is a bounded uniform 3D grid baked from the same physical surface
transport solve as the lightmap atlas: visible non-switchable emitters, reflected
surface radiance, and authored sky radiance on escaping rays. Switchable fixtures
are excluded from the base in every state. The current writer gathers each switched
atlas group's reflected surface radiance into an independent aligned probe plane;
this plane contains no source direct term, global light, escaping sky or baseline
fill. Entities add enabled planes before reconstructing their residual response,
and reserved runtime direct slots evaluate the finite sources at the actual
surface. Disabled groups add neither term. Group count is at most four; indices
are unique and must reference compiled lights. Every plane has exactly `count`
finite, nonnegative HDR means and bounded signed moments.

Every current record explicitly stores direct-sidecar presence and the
switchable group count, including zero groups. Superseded layouts are rejected;
the player never synthesizes missing bounce or rebuilds a map on load.
Material emission without an authored light source is
appearance, not transport emission.

All integers/floats are little-endian, without alignment padding: the header is
38 bytes and each sample is 36 bytes. `origin` is the **lower grid boundary**,
not the first probe center. Centers are `origin + (index + 0.5) * cell_m`, with X
fastest, then Y, then Z. Positions use world-space metres, +Y up; moments use world XYZ axes in irradiance units;
there are no additional model transforms, origin offsets or exposure factors.

`irradiance` is RGB, finite, nonnegative linear HDR in the engine's calibrated
white-surface light units; it is not a normalized color or an sRGB value.
Each channel must be at most 65504, the shared static-atlas representable range;
larger values are rejected rather than clipped. Accepted PPF1 values retain f32
precision. No gamma,
exposure, brightness clamp or half-float quantization is applied to the PPF1 record.
`direction` is the signed world XYZ sum of the per-channel first moments, with
`|direction| <= sum(irradiance)` up to floating-point rounding. `axis` is reserved;
the baker writes `[0.5, 0.5]`. Reconstruction follows `LightmapTexel::light_at`.
The compact directional fit approximates a multi-direction field; it is not a
full spherical-harmonic basis. Interpolate means and moments before reconstruction.
Never normalize the stored moment to unit length or treat its components as RGB.

A valid `room` is a nonnegative index into the compiled lighting record's room
order. Package loading rejects labels outside that room list; cross-room
sampling additionally verifies the label against the probe's actual air-volume
ownership. `-1` means unavailable: outside relevant room air, too close to an opaque
surface, inside authored solids, or detectably inside a closed prop mesh. A valid
all-zero sample means **darkness**, not missing data. Compiler labels use actual
floor/stair/ramp/region heights, ceilings and 3D wall bounds. Rooms without a valid
probe are reported in build warnings. Grid spacing starts at 1.5 m, increases to
fit 64 cells per axis, and dimensions are recomputed and centered in the bounds.
The solver first checks this original grid with the complete air-volume predicate.
When a room has no valid probe, at most 63 additional quarter-cell XYZ phases
are considered at unchanged spacing and dimensions. A selected phase must
preserve every initially covered room and increase the covered room count.
Candidates rank by covered-room count, then the ascending sorted per-room valid
probe counts, balancing the smallest populations before unrelated outdoor totals.
All bounded phases are considered after an original miss; equal scores retain the
earlier phase. An originally fully covered grid returns unchanged immediately. Preparation targets and
the baked field share that exact layout, and mismatched positions fail the solve.
This bounded phase search can still miss spaces that no tested phase can cover;
the corresponding build warning remains visible. It does not enlarge runtime
sampling support or relax opaque visibility.

Readers/writers reject non-finite values, negative energy, moments beyond their
energy bound, invalid labels/reserved axes, invalid grid headers, wrong lengths,
trailing bytes and unsupported versions. They do not repair or brighten malformed
data. Unresolvable sampling returns `None`; fallback policy belongs to runtime.
Runtime uses normalized compact Shepard weights within two grid cells:
`(1 - distance_cells / 2)^2 / distance_cells^2`, with an exact-center path.
Energy and signed moments accumulate in f64 and convert once to f32. The anchor
for a combined diagnostic trace is the transformed bind-pose model-bounds
center. Fields with a direct sidecar use eight transformed bind-bounds corners,
with compatible visible centre/corner rescue for missing support.
Actual air-volume checks reject below-floor and above-ceiling samples. Every
contributor segment is checked, including within a connected area: short
dividers and furniture can block a ray without disconnecting room air.
Compiled-solid and current movable-mesh visibility permit blending through
real openings while rejecting blocked segments.
The runtime preserves valid darkness and raw HDR shader input. Missing fields,
roomless/invalid positions and unresolved support have explicit diagnostic reasons
and use the existing authored environment sample, bounded to finite display RGB.
No white/black constant or extra brightness floor is introduced.

A present direct sidecar retains combined coefficients and selected always-on
local direct means/moments aligned to every slot. Source IDs are strictly ascending,
unique and valid indices into packaged lighting; their bounded count is at most
eight. The sidecar has no independent probe count: its length uses the shared field
`count`, and it adds `4 + 4*selected_source_count + 24*count` bytes. It contains
incident energy, with no material albedo or gamma conversion. Readers validate
nonnegative combined-minus-selected energy and the residual moment bound. A zero
selected-source count still requires all `count` coefficients, each with zero
mean and moment. This preserves spatial sampling for indirect-only and switch-only
scenes. A field may explicitly omit the sidecar and use combined centre sampling.
Unsupported or malformed records are rejected without repairing energy.

Current spatial entity sampling interpolates eight transformed model-bounds
anchors. It subtracts the selected means and moments before nonlinear directional
reconstruction, then adds those finite direct sources at actual shader fragments
once. Global direct, diffuse/sky, authored fill and unselected local direct remain
in the residual. Every probe segment requires visibility, including samples in
one connected air region; a short divider must not leak light merely because air
connects above it. Invalid corner support can reuse a compatible visible centre
or corner; unreachable support remains zero. Valid dark samples remain dark.

Solver revision 1 identifies the current solve. Recompile outputs from superseded
schemas. See
[Probe baker audit](reports/release-0.7.0.md#probe-baker-audit) for diagnostics, regression evidence and
remaining compiler/runtime limitations, and [Stage 4 contracts](art-style/stage4/contracts.md)
for current entity direct/visibility/resource behavior.

## 7. Limits

| Bound | Value |
| --- | --- |
| archive entries | 512 per ordinary map or virtual project member; 8192 physical entries in a complete project |
| manifest | 2 MiB |
| semantics | 64 MiB |
| ordinary entry (decompressed) | 256 MiB; typed records may impose tighter bounds |
| mesh / props record | 512 MiB each |
| props expanded literal record | 512 MiB, including metadata, 81-byte vertices and u16 indices |
| lightmap atlas payload | 384 MiB + 64 KiB container overhead |
| lightmap metadata | 64 MiB |
| reflection positions | 16 MiB |
| compiler provenance | 2 MiB |
| ordinary-map aggregate (decompressed) | 1 GiB; also retained for every virtual project member |
| complete-project aggregate (decompressed) | 2 GiB of unique physical records plus project index |
| ordinary-map compressed transport | 1 GiB + 64 MiB |
| complete-project compressed transport | 2 GiB + 64 MiB |
| project source recovery | 64 MiB total; existing per-file bounds retained |
| project index | 2 MiB |
| central directory | 8 MiB; ZIP64/multi-disk/encrypted entries refused |
| variants | 4 (one per lightmap quality) |
| dependencies | 4096 |
| regional and ground fog (`semantics.json` `fog_regions` + `ground_fog`) | 16 combined |
| void walls (`semantics.json` `void_walls`) | 256 |
| static mesh | the level format's own vertex budget (`MAX_LEVEL_VERTICES`, 24 000 000) |
| mesh ranges | 1 048 576 |
| mesh material index | the level material budget (`MAX_LEVEL_MATERIALS`, 131 072), or the `0xFFFFFFFF` sentinel |
| prop batches | 1 048 576 |
| prop submeshes per batch | 4096 |
| lighting record | 256 MiB |
| collision record | 128 MiB / 4 194 304 boxes |
| navigation record | 128 MiB including links / 2 097 152 XZ columns and surface nodes each / 8 classes / 256 portals |
| lightmap page edge | 4096 texels, 64 pages |
| probe face edge | 256 texels, 32 probes |

The larger complete-project allowance requires a validated bounded project
index, exact archive namespace and per-member record sum before admission; a
`project.json` filename alone is insufficient. It does not raise shared-prop
working-memory, binary-record or typed decoder budgets. Complete export streams
records through owned disk spools, while runtime decoding remains limited to the
selected member. See [project distribution and recovery](PROJECT_FORMAT.md#stage-7-project-distribution-and-editable-recovery)
for source, transport and spooling contracts.

The reader also rejects, before decoding: absolute, drive-letter, UNC or
`..`-traversing names; backslash separators; directory entries; duplicate
normalized names; control characters; symbolic links; inconsistent directory
counts, framing or unaccounted bytes; entries whose declared size exceeds its bound; declared
aggregate size over the total bound; truncation; trailing bytes; non-finite
floats; inverted bounds; out-of-range indices; wrong hashes; missing
dependencies; and unsupported versions or capabilities.

## 8. Versioning

`schema: "places-map"` and `package_format: 1` identify the one accepted
package schema. The authored semantics require `schema: "places-level"` and
`format_version: 1`. These mandatory schema markers distinguish this deliberate
pre-release reset from historical files that also carried numeric revision 1.
Binary records use their declared current signatures and revisions. Navigation
PNV2 uses revision 2. PPS1 shared props read revisions 1, 2, 3 and 4, while the writer
selects revision 2 or 4 attribute planes by measured archive cost; this preserves existing current-schema v1
recipes without accepting superseded package schemas or historical signatures.
Other superseded packages require recompilation.

A compiler fingerprint is a separate rebuild identity. Required capabilities
remain explicit and unknown names are rejected. Record/schema validation runs
independently of source, catalogue and executable currentness.

## 9. Compiler commands

```sh
places-compile build <source.json> [--out <package>] [--variants off,full]
                     [--asset-root <dir>] [--workers N] [--force] [--json]
places-compile build-collection <dir> [--variants …] [--asset-root …] [--workers N] [--force]
places-compile validate <package> [--json]      # decode every record, re-hash every entry
places-compile inspect <package> [--json]       # manifest summary and resource list
places-compile verify <source.json> --package <package> [--asset-root <dir>]
```

`--workers N` sets the shared CPU budget for the run (`PLACES_TOOL_WORKERS` is
the fallback; the flag wins). `--workers 1` selects the serial reference path.
Builds are atomic: an interrupted run leaves the last valid package in place and
a `.partial` file that the next run overwrites.

Stage 6 adds the required capability `luau-v1` for selected-map Luau snapshots.
`semantics.json` carries bounded `script_sources` records and their safe logical
paths; the player reads no project files. Older players reject this capability.
Complete project/multi-level packaging remains stage 7; see [SCRIPTING.md](SCRIPTING.md).

## Complete project packages (stage 7)

A project archive uses `project.json` instead of a root single-map manifest, with
schema `places-project-package` and `package_format: 1`. It carries ordered project
membership and the entry-level ID, maps each ordinary per-level record name to a
shared `blobs/<sha256>.record`, and indexes exact editable-source files separately.
The reader validates this outer namespace before selecting one virtual ordinary
package. It then runs the same manifest, dependency and prepared-record decoders.
No inactive level is eagerly loaded, and no sources are extracted while playing.
Older players lacking this schema refuse the archive; no existing revision changed.

Full import validates every record hash plus every level’s semantic/script/source
contract. `places-compile validate` also decodes all prepared records. Editable
recovery validates confined source roles and images in owned staging and atomically
publishes only a new destination. Recovery first copies a bounded regular input
into its exclusively reserved staging directory: all validation and recovery
passes read that one snapshot even if the caller replaces the original path.
Cancellation/failure removes only owned staging and preserves existing targets.
Ordinary compiled maps have no recovery data.

`verify --level ID --require-current` resolves compiler provenance through the
selected project's record view. Export can seed a staging map from that same
view of the previous distribution; it verifies every declared seed record,
then the ordinary compiler enforces exact source/asset/tool/variant and
prepared-stage signatures. A semantic-only edit can therefore reuse prepared
geometry, lighting and probes. Invalid seeds miss safely; changed sources and
destinations still prevent atomic final publication. JSON cache decisions name
each level and prepared/package hit or miss.
The [project contract](PROJECT_FORMAT.md#stage-7-project-distribution-and-editable-recovery)
documents the complete index, limits, transition capability, CLI and publication
rules. Built-in runtime models, textures and sound remain external installed assets;
reserved custom asset folders do not implement importing.

`shadow-only-v1` is required when a source has a shadow-only placed entity or
spawn template. Admission checks the source semantics as well as the manifest,
so older hosts refuse the feature rather than silently exposing its meshes.
This additive policy uses existing posed geometry and shadow resources; source,
mesh, prepared-record and package format versions are unchanged.

### Water motion source capability

New water exports declare `water-motion-v1`. The optional strict source
`water[].motion` object carries ripple speed/strength and horizontal flow speed/
yaw. Validated embedded source restores controls alongside unchanged compiled
collision geometry; no float control is hidden in mesh, collision or material
bytes. Older hosts reject the capability explicitly. Existing source/packages
without controls remain readable with calm ripples and zero current.


New exports containing `props[].animated_door` require `animated-door-v1`.
Validated semantic placements retain the named rigid clip and individual node
boxes; the player resolves their current poses from the installed hash-validated
model. Static collision record layout is unchanged. Older players reject the
capability before accepting a visual-only replacement for moving collision.

### Lossless storage migration and build validity (0.13.0)

`places-compile optimize-storage INPUT --out NEW.placesmap --asset-root assets`
validates all input records, runtime catalog admission and installed dependencies,
then re-encodes prepared prop storage without a geometry or lighting bake.
This explicit archival operation uses Zstandard-9; ordinary builds and exports
use Zstandard-6 to keep iteration fast. Add `--compact` for Zstandard-19 final
encoding with the same exact layout selection. It costs substantially more CPU
time and some encoder scratch RAM; it does not change lighting quality or the
8 MiB decoder history bound. Bootstrap JSON remains Deflate-9 or below.
The migration retains every variant/circuit state, exact PMP1 reconstruction
and decoded lightmap plane identity, other payload bytes, source recovery and
compiler provenance. Exact constant-tile compaction may change the atlas wire
bytes and blob references without changing its decoded values. The input is
never overwritten; a changed input, cancellation or an appearing destination
refuses publication. Project records are disk-spooled and deduplicated under the
existing entry/aggregate caps. The bounded working set still includes one map's
original/factored prop streams; it never expands the full collection at once.

`verify --level ID --json` includes `status` (`current`, `refresh`, `bake`,
`invalid`), `bake_required`, `integrity_valid` and named differences.
`map_compiler::verify_project` returns this data per declared level for later
editor presentation. A refresh updates metadata/navigation without repeating
light transport. Complete no-op exports verify source bytes and every member
payload and leave the existing archive untouched. During a partial export,
current members stream directly from the previous package into bounded disk
staging; only changed members enter the ordinary build/reuse pipeline.

Directional Static sky/sun queries use conservative derived wall, horizontal
and prop rejection trees, with the original exact leaf predicates unchanged.
Their combined storage is bounded below 24 MiB; larger inputs use the exact linear oracle. The index is rebuilt
from bounded prepared geometry on decode and is absent from wire records.
Static switch preparation shares immutable visibility/index allocations through
`Arc`; state-dependent light values, meshes and baked colours remain distinct.

Project validation totals count physical ZIP records once, including sources and
index, and deduplicate installed dependency identities across members. Per-map
logical references must not be added to that physical collection total. Static
prop preparation checks the shared cancellation token between batches of vertices;
the compiler refuses to encode or publish any interrupted partial preparation.
