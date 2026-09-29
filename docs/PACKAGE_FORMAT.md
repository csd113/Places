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
semantics.json                    the validated, authoring-prepared LevelDef
blobs/<sha256>.mesh               static geometry (binary record)
blobs/<sha256>.props              transformed static prop batches (binary record)
blobs/<sha256>.lighting           baked LevelLighting (binary record)
blobs/<sha256>.collision          static collision primitives (binary record)
blobs/<sha256>.navigation         baked cell navigation mesh (binary record)
blobs/<sha256>.lightmaps.ktx2     lightmap atlas pages (KTX2 2D array, RGBA16F, layer pairs)
blobs/<sha256>.lightmaps.json     atlas chart/stat/config record
blobs/<sha256>.probe.ktx2         one reflection probe cube (KTX2 cube, RGBA8, roughness mips)
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
  "created_by": "places-compile 0.7.0",
  "compiler_fingerprint": "<sha256>",
  "required_capabilities": ["geometry", "props", "lighting", "collision",
                            "lightmaps-hdr", "irradiance-probes", "probes-rgba8"],
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
  `irradiance-probes`, `probes-rgba8`, `collision` and `navigation`. A package
  requiring anything else is rejected by name.
* `dependencies` records the content identity of every model and texture a
  package reads from the installed asset bundle (`kind: "model"`/`"texture"`,
  root-relative path) or embeds itself (`kind: "embedded"`). A package whose
  declared dependency is missing or has a different size is refused at load
  with an error naming it (a truly asset-less install has nothing to check
  against and logs once, preserving the embedded-fallback boot).
  `places-compile validate` re-hashes every dependency and reports any content
  difference.
* `entries` must list every archive entry except `manifest.json` itself.
  Undeclared entries, duplicate declarations and declared-but-missing entries
  are errors.
* `compiler_fingerprint` is a **developer rebuild identity** — source bytes,
  dependency hashes, record versions, the variant list, the compiler version,
  the geometry revision (`render::GEOMETRY_REVISION`, bumped whenever emitted
  static geometry changes for an unchanged source), the lighting-model
  fingerprint and the transport-solver fingerprint. It is
  not a runtime validity check: a package whose fingerprint is stale still
  loads if its records satisfy this contract. `places-compile verify` compares
  the recorded fingerprint with the source and assets as they are now. A change
  to offline preparation code that is not one of those identities (for example
  the reflection-probe routing) still requires an explicit `--force` rebuild so
  stale captures are never reused.
* `lighting_fingerprint` is the **stage** identity of the illumination,
  geometry, collision and probe preparation: the same inputs as
  `compiler_fingerprint` but with every `ai`, `nav_agent` and `nav_obstacle`
  component removed. When it matches the package being replaced, the compiler
  reuses that package's prepared blobs and rebuilds only `semantics.json` and
  the navigation record, so tuning an encounter or an AI behavior never
  rebakes illumination. An absent field (an older package) simply prepares
  everything.

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

A decoded package whose prepared atlas the device cannot bind (more than eight
pages, or more than four switchable groups: a combination the compiler can
never emit) falls back to a vertex-lit rebuild of the decoded semantics. That
is the loader's defensive recovery path, not a player-side bake of the prepared
path; no compiler output reaches it, and the prepared solve never runs in the
player.

## 4. Binary records

All integers are little-endian, records are byte-aligned and every record ends
exactly at its declared end (trailing bytes are an error). Counts are bounded
before allocation; floats must be finite; indices must resolve inside their own
record.

### 4.1 Geometry — `blobs/<sha>.mesh`

```text
magic "PLMW" | version u16 = 1
range_count u32
batches 6 x i32    floor, ceiling, wall, light, prop-fallback, decal (indices)
vertex_count u32   declared total
index_count u32    declared total
ranges range_count x {
  kind u8          0 floor, 1 ceiling, 2 wall, 3 light, 4 prop fallback, 5 decal
  material u16     level material index; 0xFFFF = no level material
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
magic "PLMP" | version u16 = 3 | batch_count u32
batch {
  model u32 length + UTF-8
  bounds_min f32 x 3 | bounds_max f32 x 3
  submesh_count u32
  submeshes { texture optional u16 | alpha_mode u8 (0 opaque, 1 cutout, 2 blend)
              | alpha_cutoff f32
              | emission colour f32 x 3, intensity f32, mask optional u16
              | first_index u32 | index_count u32 }
  vertices u32 count + 69-byte vertices | indices u32 count + u16
}
```

Version 2 added the per-submesh alpha contract; version 3 added the blended
mode. A blended submesh carries opacity `1.0` in the record: an imported GLB's
`baseColorFactor` alpha is already folded into its vertices, and per-instance
fade is a runtime component, not baked content. The static prop draw path still
classifies every prop batch as opaque or cut-out; the blended flag exists so
the record reproduces the model's alpha contract exactly and the runtime
character/dynamic routes can consume it.

Submesh ranges must lie inside the batch's index list and every index must be
inside the batch's vertex list.

### 4.3 Baked lighting — `blobs/<sha>.lighting`

The complete `LevelLighting` bake: rooms, baselines, powered zones, resolved
lights, opening blends, room-to-light candidate lists, fixture-to-light
mapping, the sampling-tap count and the built static visibility set (wall,
ceiling/floor and prop occluders plus each query site's reachable-solid pool).
The player keeps this record resident because three legitimate runtime
behaviours read it:

* dynamic objects refresh their baked-light probe as they move;
* characters sample their spawn light;
* a switchable fixture re-fills exactly the lightmap charts its pool reaches.

Rebuilding this record at play time would be the static lighting bake, so it
travels in the package. The index structures (room and light lookup grids) are
rebuilt deterministically from the decoded lists; they are accelerators, not
baked content.

### 4.4 Static collision — `blobs/<sha>.collision`

```text
magic "PLCL" | version u16 = 1
walls    u32 count + (min_x,min_y,min_z,max_x,max_y,max_z,step_up) f32 x 7
floor    u32 room count + rooms (footprint, floor_y, ramps, stairs, regions)
ceiling  u32 room count + rooms (footprint, floor_y, height, profile)
water    u32 volume count + volumes (footprint, surface_y, bottom_y, opacity,
                                    swimming, optional material id)
ladders  u32 ladder count + ladders (footprint, bottom_y, top_y, facing)
```

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
{ "record_version": 2, "face_edge": 64, "levels": 7, "points": [[x, y, z], …] }
```

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
magic "PLPF" | version u16 = 2
origin f32 x 3 | cell_m f32 | dims u32 x 3 | count u32
probes count x {
  irradiance f32 x 3 | direction f32 x 3 | axis f32 x 2 | room i32
}
```

The field is a uniform 3D grid over the mapped world, baked from the same
transport solve as the lightmap atlas (every visible non-switchable emitter plus
one ray-traced diffuse gather). Switchable fixtures are excluded from the field
in every state: their contribution is prepared only as atlas layers, so
moving objects and characters are never lit by them and a toggle does not
change the field. Each probe carries the same compact linear HDR values a
lightmap texel carries (`irradiance` and the signed moment vector `direction`;
the `axis` pair is reserved and ignored) and the room it occupies; interpolation
only mixes probes of the sample's own room, so light cannot bleed through a
floor, ceiling or full-height wall, and an unresolvable position falls back to
the vertex-lit model instead of going black. A probe in no room, or inside a
wall, is never sampled. Generating the field is compile-time work; reading it is
a handful of interpolated loads.

## 7. Limits

| Bound | Value |
| --- | --- |
| archive entries | 512 |
| manifest | 2 MiB |
| semantics | 64 MiB |
| one entry (decompressed) | 256 MiB |
| aggregate (decompressed) | 1 GiB |
| variants | 4 (one per lightmap quality) |
| dependencies | 4096 |
| static mesh | the level format's own vertex budget (`MAX_LEVEL_VERTICES`) |
| mesh ranges | 1 048 576 |
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
and no compatibility mode.

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
