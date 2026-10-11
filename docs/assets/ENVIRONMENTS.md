# Environment asset profiles

These profiles preserve kit-specific artwork, mounting and source ownership.
The [catalog](../../assets/catalog.json) owns current IDs, sizes and collision
defaults; the [asset specification](../ASSET_SPECIFICATION.md) owns common
raster/UV contracts. [Map authoring](../MAP_AUTHORING_GUIDE.md) defines placement
and support, and [Offline Tooling](../OFFLINE_TOOLING.md#asset-authoring)
defines exports and checks. Paths below are repository-relative unless stated.

| Kit | Prop entries | Physical root | Maintained map / authoring owner |
| --- | ---: | --- | --- |
| Shared | 19 | `assets/shared/` | Catalog-driven Model Zoo |
| Office | 5 | `assets/environments/office/` | Places Demo / `office_refined.py` |
| Pool | 14 | `assets/environments/pool/` | Places Demo / `pool_remade.py`, `pool.py`, `duck_remade.py` |
| Home | 32 | `assets/environments/home/` | Places Demo / `home_remade.py`, `domestic_remade.py`, `tableware.py` |
| Outdoor | 89 | `assets/environments/outdoor/` | Demo, Lantern Hollow / Outdoor part modules |
| Winter | 39 | `assets/environments/winter/` | Winter / `winter.py`, `winter_village.py`, `string_lights.py` |
| Beach | 29 | `assets/environments/beach/` | Beach Demo / `beach_nature.py`, `beach_structures.py`, `beach_coast.py` |
| Frutiger Aero | 24 | `assets/environments/frutiger_aero/` | Aero Demo / `frutiger_aero.py` |
| Retro Arcade | 35 | `assets/environments/arcade/` | Cosmic Lanes & Arcade / `retro_arcade.py`, `build_retro_arcade.py` |

Counts describe the October 11, 2026 catalog. All part modules above live in
`tools/props/parts/`. Any kit can be used in any map. Native PNGs, larger masters
and intentional family duplicates remain beside their GLBs in `props/`.

## Shared props and engine images

The shared domestic/utility set includes furniture, appliances, signs and the
open-backed landscape painting frame. Its `core:` IDs retain their existing
classes. The fixed font, validation decal atlas, missing checker and emergency
white PNGs are engine resources with pinned layouts; see specification §12.3.
Diagnostic PNGs retain deliberate non-square/NPOT and alpha cases in
`assets/diagnostic/`, separate from environment artwork.

The source atlases below are opaque 256-square fitted sheets. Region order is
row-major unless stated; preserve it when exporting geometry.

| Prop | Fitted regions |
| --- | --- |
| `core:table` | top / legs / edge / apron |
| `core:bookshelf` | side / shelf / back / front |
| `core:fridge` | freezer door / fridge door / side / top |
| `core:lamp` | base / stem / shade / lining |
| `core:plant` | pot / soil / leaf / stem |
| `core:armchair`, `core:couch` | body / seat / back / wood; wood quadrant splits timber left and olive pillow fabric right, with one-pixel UV insets |
| `core:rug` | face rows 0–168, binding/backing rows 172–255, intervening gutter; native 256-square layout is fixed |

The refreshed horizontal faces map U along X and V along Z, with two-pixel
region insets. The lamp has joined hollow-shade hems and remains non-emissive;
plant leaves are closed 0.8 mm shells. Assembly intersections are intentional
where described by the builders. The painting frame uses the bookshelf sheet;
its separate decal preserves the supplied photograph. See specification §7.2
and [painting provenance](../reports/hanging-painting.md).

## Office

Reference: [Liminal Office Worlds Asset Board](../../assets/references/office/Liminal%20Office%20Worlds%20Asset%20Board.png).
The six 1024-square RGB surfaces are authoritative files loaded by
`office_art.py`, including forced exports. The wallpaper has fine vertical
pinstripes and faded lozenges; damaged paper adds soft damp fields and runs.
Carpet is dense short pile without a metre checker; damp carpet adds organic
flattened patches. The ceiling is a 2×2 acoustic-panel grid with 1 m panels;
the stained variant keeps the same grid and panel-confined tide marks.
Wallpaper/ceiling carry pale albedo for their catalog tints; carpet owns its
warm-brown colour. The fixture loads its committed opaque 1024×512, 2:1 face.

| ID | Authored construction |
| --- | --- |
| `core:desk` | bevelled laminate top, twin metal drawer pedestals, modesty panel and bridge pulls |
| `core:chair` | retained shaped seat, lumbar pad, caster base and repaired geometry; charcoal upholstery |
| `core:cabinet` | two filing drawers, reveals, label holders, bridge handles and recessed plinth |
| `core:water_cooler` | tap recess, paired controls, drip tray and continuous eight-sided ribbed bottle |
| `core:vending_machine` | recessed mountain/DRINKS fascia, controls, coin bezel and delivery lip |

Each prop retains its fitted layout, opaque 1024-square master and derived
256-square native source. `build_office_textures.py` derives those natives;
`office_refined.py` preserves the approved chair's existing geometry when
embedding art. The other four models use closed authored primitives.

One-time seam repairs remain documented in the source history: stained paper
32/12-pixel bands, radius 12, offset 16; damp carpet 80/12 bands, radius 32,
offsets 184/831; stained ceiling 16/6 bands, radius 6, offsets 16. These are
source repairs, not runtime processing. Use the seam tool's pinned parameters
and inspect the result before intentional reauthoring.

[Office refinement](../reports/office-refinement.md) retains provenance and
native review. `tests/fixtures/levels/office_asset_review.json` and
`tools/bench/capture_office.py` provide the connected review scene/cameras.

## Pool

Reference: [Quiet Indoor Pool Asset Sheet](../../assets/references/pool/Places_%20Quiet%20Indoor%20Pool%20Asset%20Sheet.png).
The kit uses pale deck ceramic, blue basin ceramic, large pale wall tile, white
resin, silver fittings and warm privacy cloth. Surface repeats retain their
construction scale: deck 10×10 tiles over 1.5 m; basin 10×10 over 1 m; wall
10×10 over 2 m; ceiling 2×2 over 2 m; coping 5×5 over 1.5 m; blue band 10×10
over 1 m. Metal trim uses the existing brushed normal at strength 0.18.
The round fixture is opaque 1024-square; the wall lens is opaque 1024×512.
The NO DIVING cutout and existing water artwork keep their contracts.

Prop atlases use fitted 2×2 cells, with 1024-square masters and 256-square
Lanczos natives. Table cells are tray/trim/leg/brace; chair seat/frame/leg/slat;
ladder tube/tread/grip/boot; curtains cloth/post/plate/track; rails
post/rail/plate/spare; bench slat/edge/metal/foot; grate metal/recess/edge/tile;
service leaf/frame/metal/dark. The duck keeps body/head/beak/eye swatches;
its eyes are closed mesh discs. The retained hot tub has a fitted two-region
128-square tile/cap sheet with inward basin normals, outward drum normals
and upward rims; its recess and water are authored separately.

Table/chair dimensions are .80×.74×.80 and .52×.85×.55 m. The .55×2.20×.45 m
ladder uses map-owned climb volumes. Rail axes remain .98 m high. Curtains
retain closed 2 mm cloth shells and at least 7.5 cm floor clearance. The
1.60×.45×.38 m bench, .60×.014×.16 m grate and 1×2.10×.15 m static service
door retain real stock depth. A static service door does not become an
operable map door merely by placement.

`pool:decal_lane_01` is a fitted 1024×32 RGBA sheet placed at 32:1 on one
flat basin floor. Coping caps are 45 mm deep with 2–4 mm joints; submerged
walking treads have real floor support and 300 mm risers. `refine_pool.py`
owns tagged Demo dressing, separate from water/climb mechanics.
[Pool reconstruction](../style-upgrade-20261007/pool/README.md) preserves
matched captures and validation; later duck geometry/art provenance is in
[the duck report](../reports/rubber-duck-remake.md).

## Home

References: [Home Environment Asset Sheet](../../assets/references/home/Home%20Environment%20Asset%20Sheet.png)
and [Warm Low-Poly Home Asset Sheet](../../assets/references/home/Places_%20Warm%20Low-Poly%20Home%20Asset%20Sheet.png).
The practical residential kit includes cream seating, shaker cabinets, low
timber tables/console, shelf books, ceramic tableware, rug, lighting and
kitchen fittings. Local warm paint/oak/ceiling variants retain the shared
originals. Backsplash has eight 15 cm tiles per 1.2 m repeat; floor tile has
four 30 cm tiles; oak has eight 20 cm boards over 1.6 m, walnut eight 15 cm
boards over 1.2 m, and patterned paper uses sage sprigs on a 25 cm cell.
Trim, white door paint, cedar and the flush-mount face keep their shared roles.

New furniture/fittings keep opaque 1024-square masters and 256-square native
atlases. Cabinets use body/door/counter/metal; seating body/seat/back/wood;
other static fittings main/secondary/detail/dark. CRT retains
screen/bezel/body/panel, with its programme fitted into the upper 75% of the
screen cell. `assets/environments/home/props/art/city_dusk_01.png` is the
retained programme source. The hanging ball light has orb/cord/rose cells,
380 mm diameter and a ceiling-contact top .8 m above its base. Wall switches
retain named nodes and the `toggle` clip. Map lights supply illumination for
lamp/strip emissive groups.

Place outlet/landscape/casing backs on actual walls with +Z toward the room.
Cabinet doors have real recesses and construction joints. The optional
`home:dining_chair_refined` keeps the .50×.902×.49 m original bounds and
existing native chair atlas; its softened stock is documented in
[Stage 6](../art-style/stage6/README.md).

`refine_home.py` owns tagged Demo dressing. The paired Adam/Eve frame and
wall decal follow specification §7.2; the full 900×546 photograph remains
inside transparent 1024-square padding.
[Home reconstruction](../style-upgrade-20261007/home/README.md) preserves
matched native evidence, including the reference's unimplemented balcony/
sliding-glazing architecture. Existing tableware, floor plant and bed sources
retain their provenance.

## Outdoor

Reference: [Quiet Night Places Concept Sheet](../../assets/references/outdoor/Quiet%20Night%20Places%20Concept%20Sheet.png).
The kit supplies lawns, gravel/concrete paths, five house families, porches,
fences, trees, lanterns and the roads/rocks/campfire used in Lantern Hollow.
Ground sheets are 512-square derivatives of 1024-square masters: grass at
2 m, gravel at 1.6 m and concrete at 2 m repeats; concrete joints form 1 m
slabs. Siding is wall stock, shingles ceiling/roof stock. Path feather decals
keep their direction, blended alpha and 2:1 edge / square end/corner layouts;
natives are 256×128/128-square, masters 1024×512/512-square.

Trees are closed opaque faceted foliage and branch meshes. `tree_02` has a
2×6.2×2 m envelope; map collision follows trunks rather than canopies. Grass
cards preserve their two-cell MASK atlases. New fitted quadrants are
bark/metal/mineral body, stock/pane, foliage/roof, alternate foliage/stone.
The `house_base` layout remains siding/trim/glass/jamb; family 02 keeps its
seven-region normalized frame and 1024 master plus 128/256-square natives.
All five house families retain their own artwork, loaded during export.

Lanterns have pane heads, corner bars, hoods and finials. Their emissive panes
need map-owned lights; use `occludes:false` for a lamp whose closed head
contains its owned emitter. The fitted stock also supports round/low bushes,
two-rail fences, masonry piers, porch canopy, road barrier, boundary ridge
and static campfire. Canopy high edge faces local −Z; the pier cap supports
the fence lantern at 1.2 m. `campfire_static` retains the animated fire's
geometry/material/source sheet with no clips. Boundary scenery needs
separate collision. The tiny `outdoor:collision_peg` carries explicitly
sized hidden support, with `occludes:false` and its mesh below the floor.

[Outdoors reconstruction](../style-upgrade-20261007/outdoors/README.md)
preserves source hashes and matched views. Historical prior tree art remains
as `tree_03_snow_base.png`; Winter uses the current canonical bare kit plus
supported snow, with independently retained original evidence.

## Winter

Reference: [Winter Expansion Environment Concept Sheet](../../assets/references/winter/Winter%20Expansion%20Environment%20Concept%20Sheet.png).
The [historical prompts](../../assets/references/winter/prompts/Winter%20Place%20Prompt.txt)
are preserved source material. The kit supplies supported snow, frozen-water
art, a stone/timber village, icicles, string lights and the aurora panorama.
`build_winter.py` owns the maintained map; the compiler prepares its lighting.

Square 1024-pixel snow/ice/masonry/timber masters remain committed. Snow
terrain is an 8 m repeat, packed snow 5 m with restrained compaction tint;
model snow/ice derivatives are 256-square. Ice is opaque albedo used by a
blend material at .84 opacity and an opaque backing; its 8 m repeat and
traction are independent contracts. A frozen pond requires a solid floor at
the waterline. `swimming:false` alone supplies no collision.
Village atlas quadrants are stone/wood/metal/amber; emission belongs only to
amber panes. Snow accumulation uses the committed white-sheet binding.

| `winter:` modules | Native bounds or mounting contract (metres) |
| --- | --- |
| `tree_snow_01`, `tree_snow_02` | 3.2×6.8×3.2, with canonical bare evergreen and supported closed bough coats; trunk collider .6×6.8×.6 |
| `drift_small`, `drift_medium`, `drift_large` | 1.2×.28×.8; 2.4×.55×1.5; 4.4×.9×2.8 |
| `drift_wall`, `drift_fence`, `mound_irregular` | 3×.65×1.1; 1.8×.38×.7; 2.1×.75×1.9 |
| `boulder_snow`, `rock_face_snow` | visuals 1.4×.97×1.3 and 4×5.16×2.2; colliders retain bare 1.4×.85×1.3 and 4×5×2.2 |
| `snow_door_overhang`, `snow_awning` | 1.6×.14×.55; 2.4×.18×2.6 |
| `snow_stair_edge`, `snow_ledge` | .38×.085×.8; 1.2×.10×.24 |
| `snow_roof_edge`, `snow_porch_edge` | 2.4×.16×.36; 1.8×.10×.3 |
| `snow_roof_slope` | 3.4×1.41×2, pitch 1.67/2.6, high at −Z |
| `snow_roof_blanket` | 6.811429×1.425174×2.228571, continuous pitched coat with supported eave lip |
| `snow_rail_top`, `snow_post_cap` | 1.2×.075×.11; .12×.085×.12 |
| `railing_snow_straight`, `railing_snow_corner`, `railing_snow_end` | 1.8×1.125×.16; .3×1.125×.16; .3×1.135×.16; retain bare rail colliders |
| `fence_post_snow` | .16×1.135×.16; original 1.05 m post plus cap |
| `icicle_short`, `icicle_medium`, `icicle_long` | .07×.18×.07; .10×.36×.10; .13×.65×.13 |
| `icicle_cluster_mixed`, `icicle_cluster_sparse` | 1.2×.65×.13 with seven spikes; 1.2×.36×.10 with three |

Cap bases sink 8–9 mm into supported stock. Slope cap source Z spans −.9..1.1;
its pivot is Z=.1, Y=.20846; preserve the cottage's 1.4 scale/yaw. Icicle roots
are at catalog height: base Y is `underside + .015 - height * scale`, measured
relative to the local rendered floor. Snow additions remain non-solid and
preserve original structural colliders, including X/Z swaps under quarter turns.
Village construction uses closed coursed masonry, timber lanterns, open
entrance frames, door hoods, window stock and angular ice. Stone is sampled
within one block; geometry owns courses and construction joints.

String spans short/medium/long retain 2.8/4/6.6 m attachment spacing,
.15/.28/.40 m sag and 3/5/7 bulbs. Bounds are 2.85×.395×.09,
4.05×.525×.09 and 6.65×.645×.09. Bulbs remain 9 cm diameter and 14 cm high;
clips are at local X=±span/2, Y=sag+.22; base Y=0 is the lowest bulb underside.
The opaque 128-square glass/cable/clip source is copied from the ball light.
Amber bulb emission is strength 1; actual map lights sit 2 cm below each bulb.
Use `tools/props/string_lights.py` for shared attachment/light coordinates.
The map's overhead/rail sources use .32/4.5 m and .10/3 m intensity/range.

The aurora retains opaque 2048×1024 equirectangular U-repeat/V-clamp artwork;
its source and exact seam correction are documented in
[aurora provenance](../reports/winter-aurora.md) and specification §26.
Calm snowfall and configurable blizzard presentation are map/environment
features. The lodge's manual strengths and timed cycle are documented in
[environment refinement](../reports/environment-refinement.md).
[Winter reconstruction](../style-upgrade-20261007/winter/README.md),
[foundation](../reports/winter-foundation.md), and the
[final audit](../reports/winter-final-audit.md) retain accepted evidence and
its limitations; their historical timings are not current performance claims.

## Beach

Reference: [Tropical Beach Concept Board](../../assets/references/beach/Low-Poly%20Tropical%20Beach%20Concept%20Board.png).
`build_beach.py` owns `levels/beach_demo.json`. The kit reconstructs the cove,
pier and raised town, with three independently rigged animals described in
[entity profiles](ENTITIES.md#beach-animals).

| Concept | Reusable resources, all `beach:` |
| --- | --- |
| B01–B02 | sand/grass, shore/cove foam and polygon-cell shallow/deep water |
| B03–B05 | rocks, islands, open sea arch, tapered lighthouse and folded-frond palms |
| B06–B09 | X-braced crate, post/beam dock, open kiosk, blue stilt house and brown hut |
| B10–B12 | coral/ivory umbrella, blue sling chair and right-arrow signpost |
| B13–B14 | cream/blue/coral town houses, arch/stairs/terrace/base/parapet, shrubs and bunting |
| B15–B18 | seagull idle/fly, crab idle/walk, fish swim, sunny panorama |

Ten opaque surface families keep 1024-square masters and seamless 512-square
runtime sheets. Sky is opaque 2048×1024 without baked landscape; the sun is
at +45° bearing/elevation, matched by `daylight()` directional lighting.
Water transparency comes from the existing material/volume controls.
The shared 1024-square master derives its fitted 256-square atlas with Lanczos:

```text
sand       grass      rock       stucco
wood       plank      palmtrunk  palmleaf
blue       coral      yellow     ivory
dark       turquoise  window     foam
```

Town coral is vertex tint over ivory; umbrellas/chair upholstery use the
fitted palette cells. Static meshes remain closed/faceted with geometric
normals. Animal masters instead use exact box-average native derivatives.

`tools/levels/beach_components.py::shore_segment` emits dry/wade/swim support:
a lowered seabed, two gentle ramp runs and shallow/deep volumes. Water X edges
inset 2 cm preserve ownership across adjoining strips; dock support must stay
outside those ramps. Decorative terrain and foam do not create support.
`beach_structures.placed_components(name, x, z, base_y=0, floor_y=0,
rotation_degrees=0, identity=None, floor_at=None)` emits the visual instance
plus `props`, `floor_regions`, `stairs`, `archways`. Base/floor heights are
absolute; pass `floor_at(x,z)` for a lowered region or ramp. Quarter turns
and unique module identities are required for supported assemblies.
Dock floors, hollow hut post/wall blockers, town pier/header stock and twelve
stair treads provide actual collision, seated 5–6 mm within visible stock.
`beach_nature.placed_components("sea_arch", ...)` provides tight pier/header
blockers; the caller owns the approach/seabed. Palm collision follows its
bent trunk. Invisible peg carriers use explicit sizes and `occludes:false`.

The connected foam shell, terrace arcade and seated bunting anchors are in
[environment refinement](../reports/environment-refinement.md).
[Beach/Aero program provenance](../reports/two-environment-implementation-plan.md)
retains original board hashes, coverage and native acceptance.

## Frutiger Aero

Reference: [Frutiger Aero Concept Sheet](../../assets/references/frutiger_aero/Frutiger%20Aero%20Places%20Concept%20Sheet.png).
`build_frutiger_aero.py` owns `levels/frutiger_aero_demo.json`.

| Reference | Reusable `frutiger_aero:` resources |
| --- | --- |
| A01–A03 | rounded wall bays, lime crown/portal stock and exactly three folded leaf-marked glass panels |
| A04–A07 | octagonal light pod with three 3.6 m hangers, tree planter, twin-seat pod and display kiosk |
| A08–A10 | open circular basin, three translucent bubbles, open barrel canopy and tall accent panel |
| A11–A13 | twelve-metre ribbed dome/soffit, ceiling ring, closed/open static double doors, curved counter and warm-light soffit |
| A14–A15 | atrium/corridor/reception banners, five decals, city/green backdrops and sunny panorama |

The 256-square shared atlas keeps its 1024-square master: sixteen palette cells
in the top quarter, four 64×128 branding panels in the middle half and
accent/leaf/ring/terminal art below. Five dedicated atlas/master pairs retain
1:2 faces in the left 128×256 native / 512×1024 master region, with
white/lime/cyan/silver stock on the right. All atlas PNGs stay opaque,
UV-clamped and +Z upright. Scalar BLEND opacity controls glass: .17/.16/.13
for panel/tube/dome; bubbles .45 with roughness .055 and metallic .12.
Four 512-square surfaces keep 1024-square masters; cyan glass has straight
alpha 52–64. Atrium/reception/kiosk decals are 512×1024; corridor/leaf 512-square,
all with transparent margins. The 2048×1024 sky's +45° sun matches its light.

`structural_components` / `placed_components` provide tight collision stock
for bays, arches/spandrels, folded panels, seats, planters, doors, tube and
counter. Hollow visuals use `solid:false`. Facet-aligned canopy roof support
uses 18 µm carrier meshes; opaque stock carriers are 1.8 mm. Explicit collider
extents remain full size. Floors are separate. Basin support lies 5 mm below
its visible slab; circular water radius is 1.30 m, surface Y=.37, bottom .11,
with `swimming:false`. The open doors are an accessible static variant.

Normal exports load committed PNGs; `frutiger_aero_art.py --author` is an
explicit offline artwork operation. The kit uses existing transparency,
lighting, water and scalar sheen; its imagery introduces no creature rig.
[Program report](../reports/two-environment-implementation-plan.md) and
[refinement](../reports/environment-refinement.md) preserve coverage and native review.
