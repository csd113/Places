# Pool — concept reconstruction, 2026-10-07

Starting commit: `295ab2f0bdf45f63f4165d6425293c3e6f9f0802`, on
`Winter-expansion`. This is the Pool entry in the [visual development journal](../README.md).
The approved source is the actual PNG inside the Pool asset directory:
[Places: Quiet Indoor Pool Asset Sheet](../../../assets/environment/pool/Places_%20Quiet%20Indoor%20Pool%20Asset%20Sheet.png).
Its pixels, the map authoring guide and asset specification were inspected
before rebuilding content. No other environment pass is included.

## What the reference establishes

The reference has a controlled hierarchy: pale commercial deck ceramic with
small mineral flecks; distinct cool blue basin ceramic with light grout;
larger pale wall tiles; smooth neutral ceiling panels with fine seams. Wear
is subtle and repeat scale is readable. The blue recessed volume, pale dry
rim and dark lane marks separate the pool's construction without noisy dirt.

Round white resin tables and armchairs have rolled edges, tapered feet,
vertical back slots and a crowned back. The ladder has two full inverted-U
chrome grabs, bolted returns and dark tread inserts. Rails are a single waist
rail on flanged posts. Curtains have slender tracks/posts, gathered tops,
deep vertical folds and a pale weighted hem. The showcase adds slim blue
pilasters, blue framed service leaves, grey four-slat benches, a plant,
submerged treads and restrained drainage/utility details. Negative space is
part of the scene: construction detail belongs at material boundaries and
functional edges, with little freestanding clutter.

## Inventory and decisions

| Existing family / component | Classification against reference | Result |
| --- | --- | --- |
| Deck PNG, `core:pool_tile_deck_01`, wet variant | Substantial texture refinement | Remade 1024² sheet, 10×10 / 1.5 m; pale glaze, fine grout, legible mineral flecks. Wet material/reflection behavior retained. |
| Basin PNG/material and floor-region skirts | Remake | Blue 10 cm ceramic replaces grey/ambiguous tile; submerged skirts use the same basin material. |
| Wall PNG/material | Remake | Pale 20 cm ceramic at a 2 m repeat; quieter mottling and grout than the original. |
| Ceiling PNG/material | Substantial refinement | Smooth neutral 1 m panels, fine seams; existing room/ceiling structure retained. |
| Basin, notched shallow shelf, room floor/ceiling and connected openings | Appropriate functional layout; substantial construction refinement | Existing dimensions and connections retained. Added dry coping, five blue submerged treads, blue wainscot, ceiling shadow line, pilasters and Pool-side casing. |
| Blank metal notice boards | Remake | Removed unbound placeholder panels; two static blue service leaves with frame, pull handle, threshold and lower vent. |
| Windows/glazing and connected doors | Minor local refinement | Existing openings, glass, shared door behavior and Office-facing artwork retained; added proud Pool-side aluminium casing/sills. |
| `core:pool_table` | Remake | Round rolled top, recessed skirt and four tapered floor-contact legs; original catalog bounds retained. |
| `core:pool_chair` | Remake | Moulded arms, crowned back with five vertical slots, rolled seat and tapered feet; original bounds retained. |
| `core:pool_ladder` | Remake | Closed inverted-U grabs, deck-return flanges and four inset non-slip treads; existing climb volume/alignment retained. |
| `core:pool_guardrail_{straight,end,corner}` | Substantial refinement | Removed the extra mid rail; slimmer single top rail at .98 m, flanged stock retained and regenerated with silver atlas. |
| `core:pool_curtain_{straight,end,corner}` | Appropriate shell; texture refinement | Existing closed pleated shell, header, hem, tabs and metal construction retained/regenerated; calmer cream cloth and fitted master atlases. |
| Round/wall light meshes | Appropriate within fixture contracts | Existing recessed can/bezel and wall housing retained. |
| Round/wall lens PNGs | Remake | 1024² opal rings and 1024×512 vertically ribbed three-band lens. Pool light tint and wall mounting height refined locally. |
| `core:decal_no_diving_01` | Appropriate | Existing legible fitted RGBA art retained; floor sign shifted 30 cm clear of the added gutter; wall sign raised 20 cm above the tile band. |
| `core:water_pool_01`, water PNG/volumes | Appropriate working system | Unchanged surface, opacity, animation, swimming and reflection conventions. |
| `core:rubber_duck`, `core:hot_tub` | Appropriate existing identity/function | Models, textures, float behavior, circular water and mechanics retained. Hot-tub open shell is intentional existing geometry. |
| Bench / overflow grate / framed service leaf | Missing | Added `pool:pool_bench`, `pool:pool_drain`, `pool:pool_service_door` with GLBs and native PNGs. |
| Coping / blue construction band / aluminium trim | Missing material separation | Added three Pool PNG/material pairs. Metal casing reuses the existing brushed normal at a restrained .18 strength; shared normal artwork is unchanged. |
| Basin lanes | Missing | Added a fitted 1024×32 RGBA navy tile stripe, placed at exact 32:1. |
| Showcase plant | Missing local placement | Reused the existing file-backed `core:plant` by the resin seating; no shared model edit. |
| Additional pipes, vents or wall inlet assemblies | No separate reference asset family | No speculative clutter. The service-leaf vent and overflow grates supply readable utility detail. Small submerged wall inlets remain a reference gap. |

The three new props are also in the Pool fixture. The catalog Zoo generator
uses previously empty south-east apron space: every pre-existing Zoo prop,
room, wall, light, route and water component is unchanged. The connected demo
retains all water/climb definitions, entities and non-Pool rooms/materials.
Only its Pool static dressing and Pool fixture settings change.

## Authoring sources and reproducibility

| Source | Responsibility |
| --- | --- |
| `tools/textures/author_pool.py` | Offline Pillow authoring of the seven surface sheets, two fitted fixture faces, lane marking and twelve model atlases. |
| `tools/textures/pool_art.py`, Pool functions in `lights_art.py` | Normal texture pipeline loads committed PNGs, including forced rebuilds, instead of repainting them with the old small painters. |
| `tools/textures/water_art.py`, `decal_art.py` | Existing water and NO DIVING sources retained. |
| `tools/props/parts/pool_remade.py`, registry in `pool.py` | Closed rail/curtain construction and rebuilt table, chair, ladder, bench, grate and service leaf. Old unused table/chair/ladder helpers remain historical source only. |
| Existing `duck_remade.py` and `pool.py` hot-tub builder | Retained duck and circular shell, outside this remake. |
| `tools/levels/refine_pool.py` | Idempotent tagged Pool dressing in `assets/levels/places_demo.json`; no compiler/runtime texture generation. |
| `tools/levels/build_model_zoo.py` | Minimal three-prop Pool apron registration without reflowing existing exhibits. |
| `tools/bench/capture_pool.py` | Fixed native renderer cameras/settings used for both stages. |

Each remade/new model retains a 1024² `*_master.png`, downsampled once with
Lanczos to its fitted 256² native atlas. GLBs embed only the native image.
High uses native model atlases; Low reduces these to 128². Surface PNGs remain
1024² authored sources under the existing runtime quality policy. All imagery
is committed under `assets/`; no painter runs at game startup or level load.
Mandatory aspect ratios, alpha, orientation, bounds and fitted UV layout are
preserved and documented in the [Pool asset README](../../../assets/environment/pool/README.md)
and [asset specification](../../ASSET_SPECIFICATION.md).

## Matched native comparison

These are unedited captures of the actual SDL/wgpu Metal renderer, not asset
previews or composited mockups. Click any image for the full 1280×720 PNG.

| View | Before | After | What changed |
| --- | --- | --- | --- |
| Wide Pool | ![Wide before](before/wide.png) | ![Wide after](after/wide.png) | Blue recess, pale deck/walls, single rails, coherent construction boundaries and calmer palette. |
| Basin | ![Basin before](before/basin.png) | ![Basin after](after/basin.png) | Readable blue ceramic, navy lanes and basin-edge separation. |
| Walls | ![Walls before](before/walls.png) | ![Walls after](after/walls.png) | Larger pale tiles, blue wainscot, framed glass and slender blue piers. |
| Ceiling / windows | ![Ceiling before](before/ceiling_windows.png) | ![Ceiling after](after/ceiling_windows.png) | Neutral fine-seamed ceiling, opal fixtures, casing and construction shadow line. |
| Water edge | ![Edge before](before/edge.png) | ![Edge after](after/edge.png) | Pale dry coping, repeated grate segments, blue walls and submerged stair silhouette. |
| Connected steps / service leaf | ![Steps before](before/stairs.png) | ![Steps after](after/stairs.png) | Existing walkable connected steps retained, utility leaf replaces flat panel. |
| Ladder from water | ![Ladder before](before/ladder.png) | ![Ladder after](after/ladder.png) | Closed chrome stock and darker tread inserts against blue tile. |
| Furniture | ![Furniture before](before/furniture.png) | ![Furniture after](after/furniture.png) | Round resin table, arms and vertically slotted crowned chair back. |
| Curtains | ![Curtains before](before/curtains.png) | ![Curtains after](after/curtains.png) | Cream folds/header/hem, slender metal supports and missing bench. |
| Close deck / coping | ![Tile before](before/tile_close.png) | ![Tile after](after/tile_close.png) | Quiet mineral flecks survive runtime sampling; larger coping separates deck from recess. |
| Submerged treads | ![Basin stairs before](before/basin_stairs.png) | ![Basin stairs after](after/basin_stairs.png) | Five real 300 mm risers rather than an undecorated basin edge. |
| Complete ladder | ![Ladder deck before](before/ladder_deck.png) | ![Ladder deck after](after/ladder_deck.png) | Full inverted-U grabs, bolted returns and tread proportions from the dry deck. |
| Major new prop | ![Bench before](before/bench.png) | ![Bench after](after/bench.png) | Four-slat trestle bench occupies reference-appropriate changing-bay space. |

Camera positions and angles are identical in each pair. `PLACES_SPAWN` is
`x,z,yaw`; the unchanged floor resolver sets eye height. `PLACES_CAMERA` is
`yaw,pitch` in degrees. [Before](before/manifest.json) and
[after](after/manifest.json) manifests give every camera, renderer identity
and PNG SHA-256. Settings: High, full lightmaps/reflections, High filtering,
bloom on, 60° FOV, 640×360 logical window / 1280×720 drawable, vsync off;
15 warmup frames, a 100-frame cap, capture/exit at frame 60 (45 CSV samples). The comparison
uses the same renderer implementation and Apple M2 Pro Metal adapter.

The first native candidate exposed a sign/gutter intersection and casing that
narrowed hot-tub egress. Both signs were moved clear of the grate/tile band. Window
casing became collision-free surface trim, with separated joints, retaining
the construction depth while restoring the exact existing water exit test.
Coping corners also received real 2–4 mm construction joints so adjacent caps
do not share coplanar faces.

The result is substantially closer in silhouette, palette and construction
hierarchy. The connected playable room retains its larger footprint and
notched shelf; it cannot become the concept's isolated 9×4.5 m pool without
rearranging connected environments. Runtime baked light makes resin/metal
darker than the concept's beauty lighting. Chrome uses fitted highlight
artwork within the existing prop material pipeline. The wall fixture keeps
its required 2:1 face rather than the taller concept housing. Bloom softens
fine lens bands at a distance. Existing water has no physical refraction;
its caustic/ripple system is retained. Small submerged wall inlets are absent.
The raised coping conceals part of the ladder's low return flange geometry.
These are recorded limits, with no global water/lighting/movement rewrite.

## Cost and technical evidence

[Model audit](model-audit.json) records bounds, topology, UV area, winding,
texture alpha/dimensions and file hashes. [Costs](costs.json) records per-family
triangles, source sizes and the native comparison counters.

Twelve remade/new GLBs total **4,168 triangles**, versus **3,354** across the
nine replaced families (+814, including the three missing props). Their disk
size falls **775,368 → 426,688 bytes**. Every remade model is at most 612
triangles. Existing hot tub and duck are excluded from that delta.

The wide view's native draw count rises **334 → 351** (+17); total loaded
vertices rise **477,755 → 495,981** (+3.8%). World texture residency rises
**205,520,848 → 216,705,656 bytes** (+10.67 MiB); surface image edge limits
stay at 1024 and the full lightmap atlas stays at eight pages. Model source
images remain 256²; the three new families add .75 MiB of base-level decoded
atlas pixels before mipmaps. The compiled demo grows **96,267,473 →
98,762,970 bytes** (+2.6%) and the final normal bake takes 223.951 seconds
with eight workers. Repeated grate and bench instances reuse model assets.
Both stages use one reflection pass. Short frame samples are retained in the
cost JSON but are not a controlled FPS claim.

All modified models have zero degenerates, boundary edges, inconsistent
winding, duplicate triangle faces, coincident quads and zero-area UV faces.
The grate has six and service leaf two position-welded assembly junction
edges: individually closed stock components share contact edges, with
separate GLB vertices and no duplicated surface. No open mesh or winding
repair is hidden by a blanket allow. Existing intentional hot-tub boundaries
are retained. Fitted atlases avoid world tiling on furniture; periodic surface
sheets pass the seam gate. Native captures were inspected for floating feet,
rim/ladder alignment, grout scale, clipping, holes and flicker.

## Validation and retained review payload

See [validation](validation.md) for exact final command outcomes and package
identities. Catalog validation, focused Pool tests, texture seam/dimension
checks, model integrity, Zoo generator currency and the demo geometry gate
are required. Rust formatting, configured Clippy and workspace tests are
also run. Only the demo and two shipped packages directly embedding changed
Pool assets are rebaked; unrelated historical packages remain untouched.

Independent native before/after payloads and raw logs are preserved outside
`target` at [debug-maps/pool-style-20261007](../../../debug-maps/pool-style-20261007/README.md).
Historical debug maps and external Consolidation/Office evidence are intact.
Reusable Cargo outputs remain available for the next serial pass. Active
caffeinate protection is handed off with the checkout rather than stopped.
