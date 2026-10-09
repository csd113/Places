# Compiled map packages (`.placesmap`)

A `.placesmap` file is the only thing a player loads. It is a ZIP archive
containing one validated level's gameplay semantics and the prepared static
world the offline compiler produced from an authoring source. The player does
not compile maps, bake light, pack charts, capture reflections or generate
static navigation; it decodes these records and uploads them.

The authoring workflow is explicit:

```sh
# edit a source, then
./target/release/places-compile build assets/levels/places_demo.json
# and launch
./target/release/places
```

`places-compile` reuses an existing package when the source, the referenced
asset identities and the compiler version are unchanged; `--force` rebuilds.
A failed build leaves the previous package untouched.

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
blobs/<sha256>.lightmaps.ktx2     lightmap atlas pages (KTX2 2D array, RGBA16F, layer pairs)
blobs/<sha256>.lightmaps.json     atlas chart/stat/config record
blobs/<sha256>.probe.ktx2         one reflection probe cube (KTX2 cube, RGBA16F, roughness mips)
blobs/<sha256>.probes.json        probe positions and mip level count for one face size
blobs/<sha256>.irradiance         prepared irradiance field for moving objects (binary)
blobs/<sha256>.png                embedded texture pixels (community packages)
```

Every blob name embeds the SHA-256 of the entry's uncompressed bytes. Readers
recompute the digest before decoding and refuse a mismatch. Entries are written
in sorted order with a fixed timestamp, so two builds of the same content
produce the same archive.

`semantics.json` is the **only** JSON a level author can influence. It is a
`LevelDef` (current authored schema: `format_version: 3`, typed components,
event bindings, actions, conditions, sequences, timers and spawn records)
after `prepare_level` (fixture alignment, decal snapping, generated baseboards);
the player validates it again but never emits geometry from it. Runtime JSON
never triggers procedural geometry generation.

## 2. Manifest

```json
{
  "package_format": 1,
  "id": "places_demo",
  "name": "Places Demo",
  "author": "Places",
  "created_by": "places-compile 0.8.0",
  "compiler_fingerprint": "<sha256>",
  "required_capabilities": ["geometry", "props", "lighting", "collision",
                            "lightmaps-hdr", "irradiance-probes", "probes-hdr"],
  "dependencies": [
    { "kind": "model", "path": "core/props/models/table.glb",
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
  `irradiance-probes`, `probes-hdr`, legacy `probes-rgba8`, `collision` and
  `navigation`. A package
  requiring anything else is rejected by name.
* `dependencies` records the content identity of every model and texture a
  package reads from the installed asset bundle (`kind: "model"`/`"texture"`,
  root-relative path) or embeds itself (`kind: "embedded"`). A package whose
  declared dependency is missing or has a different size or SHA-256 is refused
  at load with an error naming it (a truly asset-less install has nothing to check
  against and logs once, preserving the embedded-fallback boot).
  `places-compile validate` re-hashes every dependency and reports any content
  difference. Models include every placed prop and every runtime spawn template,
  including unused templates selectable by an override. GLBs embed their PNG
  images under the existing importer contract, so the model hash covers that
  runtime image closure. The player streams installed dependency hashes during
  package-open, with an 8 KiB buffer; rendering does not hash them per frame.
* `entries` must list every archive entry except `manifest.json` itself.
  Undeclared entries, duplicate declarations and declared-but-missing entries
  are errors.
* `compiler_fingerprint` is a **developer rebuild identity**: exact source
  and catalogue bytes, dependency hashes, record versions, requested variants,
  geometry/light-model/solver revisions, installed compiler executable SHA-256
  and reflection-capture mode. The executable is hashed once per process using
  bounded streaming storage. A changed tool rebuilds automatically, including
  preparation changes outside manually versioned subsystems. Runtime format
  validity remains separate from this compiler identity.
* `build-inputs.json` is an optional declared entry with role `compiler-inputs`:
  revision 1, `source_sha256`, `catalog_sha256`, `tool_sha256` and
  `capture_probes`. Older players ignore it under the existing format. Missing,
  corrupt or incompatible provenance requires a full compiler rebuild; older
  otherwise valid packages remain loadable with their compatible assets. New
  packages also verify the recorded catalogue hash at player package-open; a
  changed catalogue requires rebuilding. Legacy packages without this optional
  record retain their established bundle contract.
* `lighting_fingerprint` identifies prepared geometry, collision, lighting and
  reflections. It retains source materials/images/models, physical environment
  and fog, lights, entities, tool/capture identity, formats and quality. It
  excludes display name/author, navigation/AI components and
  `environment.presentation` (exposure, shoulder and grade after HDR capture).
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

A package carries up to one variant per lightmap quality (`off`, `medium`,
`full`). Each variant is a complete prepared world for that setting:

* `off` is the historical vertex-lit build: prepared geometry with baked vertex
  colours and no atlas. It is a supported, intentional variant, not an empty
  package.
* `medium` and `full` carry prepared geometry plus the atlas pages the
  corresponding lightmap configuration produced.
* A variant whose atlas could not be filled records `lightmap_failure` and
  ships the vertex-lit geometry instead, exactly as the runtime used to fall
  back — the fallback is recorded at compile time, never re-derived at load.

`quality_profile` records the bake/plan profile (`low` for `off`, `full`
otherwise) so a future reader can tell which bake produced the vertex colours.

A decoded package whose prepared atlas the device cannot bind (more than eleven
pages, or more than four switchable groups: a combination the compiler can
never emit) falls back to a vertex-lit rebuild of the decoded semantics. That
is the loader's defensive recovery path, not a player-side bake of the prepared
path; no compiler output reaches it, and the prepared solve never runs in the
player.

Full's profile planning cap is eleven pages; lower profiles retain eight.
Before geometry emission, the planner also bounds that cap by the actual base
and switchable illumination groups, the typed record bytes (including the exact
196-byte single-mip KTX2 header), the codec payload/layer bounds and the decoder
page count. At 1024px, Full allows 11/10/6/5/4 pages for zero through four
switchable lights. Demo therefore retains ten pages for its two groups. The
effective config is shared by the plan, fill and public cache-key refresh.
The model-lighting correction shares compatible coplanar source-triangle charts
and gives small opaque props 32 intervals/m (Medium24/m). Its measured Hallows
Full layout needs eleven 1024px pages, or 176 MiB for the base group, preserving
the finer physical shadow field. The earlier
[Stage 7 Hallows audit](art-style/stage7/integration-contract-audit.md)
established ten pages at its former sampling density. The independent typed
atlas bound remains 320 MiB +64 KiB,
the measured ten-page two-group Demo payload plus container allowance. The
[record audit](art-style/stage7/integration-contract-audit.md) records
the old 256 MiB rejection and 25% increase. The 64-page decoder count, exact
KTX2 shape/length checks, four-switch-group limit and 1 GiB aggregate remain
enforced. More groups still have to fit the typed byte bound; eleven pages do not
authorize an arbitrary total allocation. The aggregate guard still applies to
the complete archive, including all quality variants.

## 4. Binary records

All integers are little-endian, records are byte-aligned and every record ends
exactly at its declared end (trailing bytes are an error). Counts are bounded
before allocation; floats must be finite; indices must resolve inside their own
record.

### 4.1 Geometry — `blobs/<sha>.mesh`

```text
magic "PLMW" | version u16 = 2
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
  vertices u32 count, then 69-byte vertices
  indices u32 count, then u16 indices
}
vertex (69 bytes):
  pos f32 x 3 | color f32 x 4 | uv f32 x 2 | normal f32 x 3 | tangent f32 x 3
  | handedness f32 | lightmap u16 x 2 | lightmap_page u8
```

**Version history.** Version 1 wrote the range's `material` as a `u16`;
version 2 widened it to `u32`, because the in-memory material index is 32 bits
and a level may declare far more materials than a `u16` names
(`MAX_LEVEL_MATERIALS`, 131 072). A version-1 record is refused by name — its
`material` field would be misread as the first half of a 32-bit index — so
packages built by the previous compiler must be rebuilt. A non-sentinel
material above the level budget is refused too.

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
magic "PLMP" | version u16 = 6 | batch_count u32
batch {
  model u32 length + UTF-8
  casts_static_lighting u8 (strict 0 or 1)
  bounds_min f32 x 3 | bounds_max f32 x 3
  submesh_count u32
  submeshes { texture optional u16 | alpha_mode u8 (0 opaque, 1 cutout, 2 blend)
              | alpha_cutoff f32
              | emission colour f32 x 3, intensity f32, mask optional u16
              | specular f32 x 3 (linear), roughness f32
              | first_index u32 | index_count u32 }
  vertex_count u32 | vertex_mode u8
  if vertex_mode = 1 or 2: frame_count u32 + frame_count x 28-byte frame
  vertices vertex_count x vertex_storage
  indices u32 count + u16
}
frame (28 bytes): normal f32 x 3 | tangent f32 x 3 | handedness f32
vertex_storage:
  mode 0: literal 69-byte neutral vertex from section 4.1
  mode 1: pos/color/UV prefix (36 bytes) | lightmap/page tail (5 bytes) | frame u16
  mode 2: the same prefix and tail | frame u32
```

Version 6 shares only identical complete 69-byte vertices within their own
batch, keeping first-occurrence order and remapping every ordered triangle
index. It retains distinct normals, tangents, handedness, colour, UV and atlas
seams, including signed-zero float bits. Unique unreferenced vertices remain.
The frame palette also uses exact bytes and first-occurrence order. A batch
uses a palette only when its count/header/frames/references are strictly smaller
than literals; ties retain literal storage. Mode 1 requires at most 65,536
frames, otherwise mode 2 uses u32 references. The decoder reconstructs the same
neutral runtime vertices, without changing any value, shader or material.

Frame counts cannot exceed vertex counts. The decoder bounds complete payload
runs before allocation, rejects unknown modes, invalid references, truncation
and nonfinite frames (including unused ones), and retains the existing vertex
and index limits. Both writer and reader enforce a cumulative 512-MiB expanded
literal-record budget: ten record-header bytes, unchanged batch metadata through
the vertex count, 69 bytes per retained vertex, and four index-count bytes plus
two bytes per index. This makes the former literal allocation envelope explicit;
the encoded typed record still has its independent 512-MiB limit.

Version 2 added the per-submesh alpha contract; version 3 added the blended
mode. A blended submesh carries opacity `1.0` in the record: an imported GLB's
`baseColorFactor` alpha is already folded into its vertices, and per-instance
fade is a runtime component, not baked content. The static prop draw path still
classifies every prop batch as opaque or cut-out; the blended flag exists so
the record reproduces the model's alpha contract exactly and the runtime
character/dynamic routes can consume it.

Submesh ranges must lie inside the batch's index list and every index must be
inside the batch's vertex list.

Version 4 adds scalar material response. Version 5 records whether the retained
bind-pose batch casts static lighting. The same finite-placement, animator and
128-character claim plan drives character spawning and this flag: a fully claimed
model has flag 0, so its moving actors do not leave fixed bind-pose casters or
reject their own probe rays. Partial/overflow model groups retain their visible
static fallback and flag 1. Collision and model/vertex budget charges are unchanged.
Readers accept literal versions 3, 4 and 5. Versions 3 and 4 retain conservative
caster flag 1; version 3 has the explicit matte material default. Version 5's
explicit caster flag remains authoritative. Current records add a vertex-mode
byte per batch and optional palette storage; no package-major change is needed.

### 4.3 Baked lighting — `blobs/<sha>.lighting`

The complete `LevelLighting` bake: rooms, baselines, powered zones, resolved
lights, opening blends, room-to-light candidate lists, fixture-to-light
mapping, the sampling-tap count and the built static visibility set (wall,
ceiling/floor and prop occluders plus each query site's reachable-solid pool).
The compiled lighting record is version 2. Immediately after the local-light
list it stores a `u8` global count (maximum eight), then each active/baked
directional source: normalized incoming XYZ and RGB (`f32` triples), intensity
(`f32`), shadow flag (`bool8`), and angular radius in radians (`f32`). The reader
validates finite unit directions and bounded energy/angle. Atlas layout version
13 and transport revision 16 invalidate earlier prepared lighting identities.
Revision 16 uses one strictly validated, deterministically chosen probe lattice
for both prepared targets and solve positions; the source map remains format 3.
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
magic "PLCL" | version u16 = 2
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
bounding box (`radius` must equal half of both extents). Version 1 is refused
by name: its `swimming` byte would be misread as the shape code. A prepared
package therefore always holds the exact footprint the player queries.

Interactive semantics (doors, interactables, trigger volumes, entity routes,
lights, timers, sequences and spawns) stay derived from the validated semantic
record: they are O(authored entities) gameplay state, not static collision. The
player loads the record's typed component/binding/action data into the entity
runtime and instantiates only the dynamic state each object needs; the prebuilt
`CollisionIndex` grid is rebuilt from the decoded wall boxes at start.

### 4.5 Baked navigation — `blobs/<sha>.navigation`

```text
magic "PLNV" | version u16 = 1
cell_m          f32
origin_x, origin_z   f32 x 2
cells_x, cells_z     u32 x 2
class_count     u32, then per class:
  radius, height, step_height, max_slope   f32 x 4
portal_count    u32, then per portal:
  door id (u32 length + UTF-8, <= 256)
cells_x * cells_z cells, row-major:
  y             f32   (walking-surface height)
  flags         u8    (bit 0: a surface exists; bit 1: continuous slope)
  headroom_cm   u16   (u16::MAX = no overhead found)
  portal        u16   (u16::MAX = not a door-portal cell)
per class:
  walkable      u32 byte length + ceil(cells/8) bits (bit i = cell i)
  region        u32 count + u16 x cells (u16::MAX = no region)
```

The mesh is a uniform cell grid over the level's floor footprint with a
**per-class clearance mask**, baked offline from the same walkable surfaces the
movement controller follows and the same collision boxes players collide with.
A class is one physical body (`radius`, `height`, `step_height`, `max_slope`):
the reference humanoid plus every distinct `nav_agent` body the level authors.
A cell marked walkable for a class is physically traversable by exactly that
body: clearance, headroom and step/slope are all evaluated at bake time, and a
neighbour connects within one step or along a continuous slope within the
class bound. Bit 1 of a cell's flags marks a **continuous** slope: the surface
gradient is consistent across the cell, as a ramp or a staircase's pitch line
is. A discrete riser — a floor-region step, a stair's first nosing, a ledge —
puts a half-cell sample across a jump, so it is never a slope and the class's
own `step_height` alone decides whether it connects. A stair's pitch line is
continuous, so a class climbs a flight whose risers fit its step; a floor rim
never connects.

Door leaves are dynamic and are never baked as static obstacles. The cells a
leaf sweeps between its closed and open poses are recorded as that door's
portal; the runtime blocks them while the door is closed or locked and
re-evaluates them when it opens. Opening a door never rebuilds the mesh, and a
locked door is never crossed. The record is portable and explicit: bounds,
grid dimensions, class bodies, per-cell surface/headroom/portal data, class
masks and region labels are all validated before use. A malformed record fails
the load by name; there is no runtime bake and no repair path.

## 5. Lightmap pages — KTX2

`blobs/<sha>.lightmaps.json` holds the chart record:

```json
{ "record_version": 3, "page_edge": 1024, "page_count": 2, "padding": 2,
  "content_key": "v12-…",
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
interpolation cannot produce the octahedral-axis seam of record version 2),
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
{ "record_version": 3, "face_edge": 64, "levels": 7, "points": [[x, y, z], …] }
```

Positions version 3 identifies linear HDR RGBA16F cubes. Version 2 remains
readable with legacy display RGBA8 cubes, decoded once into linear values at
load; already clipped highlight energy cannot be recovered. Positions version 1
is rejected. The current writer declares `probes-hdr` and retains authored HDR
through capture and prefiltering, before runtime presentation.

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
magic "PLPF" | version u16 = 3 (version 2 remains supported)
origin f32 x 3 | cell_m f32 | dims u32 x 3 | count u32
probes count x {
  irradiance f32 x 3 | direction f32 x 3 | axis f32 x 2 | room i32
}
selected_source_count u32 | selected_source_indices u32 x selected_source_count
selected_local_direct count x { irradiance f32 x 3 | direction f32 x 3 }
```

The field is a bounded uniform 3D grid baked from the same physical surface
transport solve as the lightmap atlas: visible non-switchable emitters, reflected
surface radiance, and authored sky radiance on escaping rays. Switchable fixtures
are excluded in every state; static surfaces use their atlas layers and v3 entity
lighting uses reserved runtime direct slots. Their diffuse bounce is not stored
in the base entity field. Material emission without an authored light source is
appearance, not transport emission.

All integers/floats are little-endian, without alignment padding: the header is
38 bytes and each sample is 36 bytes. `origin` is the **lower grid boundary**,
not the first probe center. Centers are `origin + (index + 0.5) * cell_m`, with X
fastest, then Y, then Z. Positions use world-space metres, +Y up; moments use world XYZ axes in irradiance units;
there are no additional model transforms, origin offsets or exposure factors.

`irradiance` is RGB, finite, nonnegative linear HDR in the engine's calibrated
white-surface light units; it is not a normalized color or an sRGB value.
Each channel must be at most 65504, the shared static-atlas representable range;
larger values are rejected rather than clipped. Accepted PLPF values retain f32
precision. No gamma,
exposure, brightness clamp or half-float quantization is applied to the PLPF record.
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
Solver 16 first checks this original grid with the complete air-volume predicate.
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
is the transformed bind-pose model-bounds center for the legacy v2 path and
combined diagnostic trace. The v3 spatial path uses eight transformed bind-bounds
corners, with compatible visible centre/corner rescue for missing support.
Actual air-volume checks reject below-floor and above-ceiling samples. Every
contributor segment is checked, including within a connected area: short
dividers and furniture can block a ray without disconnecting room air.
Compiled-solid and current movable-mesh visibility permit blending through
real openings while rejecting blocked segments.
The runtime preserves valid darkness and raw HDR shader input. Missing fields,
roomless/invalid positions and unresolved support have explicit diagnostic reasons
and use the existing authored environment sample, bounded to finite display RGB.
No white/black constant or extra brightness floor is introduced.

Version 3 retains the original combined coefficients and adds selected always-on
local direct means/moments aligned to every slot. Source IDs are strictly ascending,
unique and valid indices into packaged lighting; their bounded count is at most
eight. The sidecar has no independent probe count: its length uses the shared field
`count`, and it adds `4 + 4*selected_source_count + 24*count` bytes. It contains
incident energy, with no material albedo or gamma conversion. Readers validate
nonnegative combined-minus-selected energy and the residual moment bound. A zero
selected-source count still requires all `count` coefficients, each with zero
mean and moment. This preserves the v3 spatial contract for indirect-only and
switch-only scenes; it does not encode switched energy in the baked base. Version
2 has no sidecar, keeps the combined centre-sample runtime path and re-encodes as
version 2. Unsupported/malformed records are rejected without repairing energy.

Current spatial entity sampling interpolates eight transformed model-bounds
anchors. It subtracts the selected means and moments before nonlinear directional
reconstruction, then adds those finite direct sources at actual shader fragments
once. Global direct, diffuse/sky, authored fill and unselected local direct remain
in the residual. Every probe segment requires visibility, including samples in
one connected air region; a short divider must not leak light merely because air
connects above it. Invalid corner support can reuse a compatible visible centre
or corner; unreachable support remains zero. Valid dark samples remain dark.

Solver revision 16 changes the bake identity and invalidates package/lightmap
fingerprints; rebuild current outputs. See
[Probe baker audit](PROBE_BAKER_AUDIT.md) for diagnostics, regression evidence and
remaining compiler/runtime limitations, and [Stage 4 contracts](art-style/stage4/contracts.md)
for current entity direct/visibility/resource behavior.

## 7. Limits

| Bound | Value |
| --- | --- |
| archive entries | 512 |
| manifest | 2 MiB |
| semantics | 64 MiB |
| ordinary entry (decompressed) | 256 MiB; typed records may impose tighter bounds |
| mesh / props record | 512 MiB each |
| props expanded literal record | 512 MiB, including metadata, 69-byte vertices and u16 indices |
| lightmap atlas payload | 320 MiB + 64 KiB container overhead |
| lightmap metadata | 64 MiB |
| reflection positions | 16 MiB |
| compiler provenance | 2 MiB |
| aggregate (decompressed) | 1 GiB |
| variants | 4 (one per lightmap quality) |
| dependencies | 4096 |
| fog regions (`semantics.json` `fog_regions`) | 16 |
| void walls (`semantics.json` `void_walls`) | 256 |
| static mesh | the level format's own vertex budget (`MAX_LEVEL_VERTICES`, 24 000 000) |
| mesh ranges | 1 048 576 |
| mesh material index | the level material budget (`MAX_LEVEL_MATERIALS`, 131 072), or the `0xFFFFFFFF` sentinel |
| prop batches | 1 048 576 |
| prop submeshes per batch | 4096 |
| lighting record | 256 MiB |
| collision record | 128 MiB / 4 194 304 boxes |
| navigation record | 128 MiB / 2 097 152 cells / 8 classes / 256 portals |
| lightmap page edge | 4096 texels, 64 pages |
| probe face edge | 256 texels, 32 probes |

The reader also rejects, before decoding: absolute, drive-letter, UNC or
`..`-traversing names; backslash separators; directory entries; duplicate
normalized names; entries whose declared size exceeds its bound; declared
aggregate size over the total bound; truncation; trailing bytes; non-finite
floats; inverted bounds; out-of-range indices; wrong hashes; missing
dependencies; and unsupported versions or capabilities.

## 8. Versioning

`package_format: 1` is the one accepted version. A newer **compiler** is not a
format change: rebuild fingerprints are separate from runtime validity, so a
package built by a newer tool still loads when its records satisfy this
contract. Adding a required capability is a format change only for runtimes
that do not understand it: they reject it by name. There is no legacy reader
for another package major. Individual accepted record revisions retain the
explicit legacy readers described above; record compatibility does not establish
source/catalogue/compiler currentness.

## 9. Compiler commands

```sh
places-compile build <source.json> [--out <package>] [--variants off,medium,full]
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
