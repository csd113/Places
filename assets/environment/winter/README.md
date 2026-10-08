# Winter asset set

The Winter Place is `assets/levels/winter.json`, authored by
`tools/levels/build_winter.py`, and shipped as `winter.placesmap`. The `winter`
catalogue theme and `winter:*` material IDs use the ordinary asset registry;
there is no separate loader or runtime image generation.

Read `docs/ASSET_SPECIFICATION.md` and `docs/MAP_AUTHORING_GUIDE.md` before
introducing winter art. Winter resources belong under this directory:

| Resource | Canonical location / contract |
| --- | --- |
| Snow terrain, packed paths, ice | `textures/floors/`; square tileable real PNGs, preferred source 1024², world-aligned UVs |
| Building snow surfaces | `textures/walls/` or `textures/ceilings/` as appropriate; ordinary tiled PNG contracts |
| Snow trees, rocks, drifts, rails, entrances, icicles, string lights | `props/models/`; GLB and sibling atlas PNGs; centered X/Z, base at Y=0, +Z front |
| Aurora | `textures/sky/`; 2:1 equirectangular PNG with seamless wrap and canonical orientation |
| Weather artwork | `textures/effects/`; establish any new class in the asset specification before authoring |
| Tracks, shoreline/detail overlays | `decals/`; real PNGs, documented coverage/alpha and aspect ratio |

Directories are created when their first real resource is added. Shared assets
stay in their current locations, with references by logical ID. Do not register
another file asset pointing at the same GLB or PNG.

## Snow-covered static kit

`tools/props/parts/winter.py` registers 28 ordinary prop builders. They load
committed PNGs and shipped base models; they never regenerate texture imagery.
Build selected winter IDs through `python3 tools/props/build.py --only <ids>`.
The level generator remains deterministic (`tools/levels/build_winter.py`).

| IDs (prefix `winter:`) | Size `[w,h,d]` metres / purpose |
| --- | --- |
| `tree_snow_01`, `tree_snow_02` | `[3.2,6.8,3.2]`; lighter/heavier irregular snow loads |
| `drift_small`, `drift_medium`, `drift_large` | `[1.2,.28,.8]`, `[2.4,.55,1.5]`, `[4.4,.9,2.8]` |
| `drift_wall`, `drift_fence`, `mound_irregular` | `[3,.65,1.1]`, `[1.8,.38,.7]`, `[2.1,.75,1.9]`; flat back on wall/fence drift |
| `boulder_snow`, `rock_face_snow` | `[1.4,.97,1.3]`, `[4,5.16,2.2]`; original rocks plus upward caps |
| `snow_door_overhang`, `snow_awning` | `[1.6,.14,.55]`, `[2.4,.18,2.6]`; hoods and terrace roof |
| `snow_stair_edge`, `snow_ledge` | `[.38,.085,.8]`, `[1.2,.10,.24]`; cleared stair centres, window ledges |
| `snow_roof_edge`, `snow_porch_edge` | `[2.4,.16,.36]`, `[1.8,.10,.3]`; shallow connected irregular crowns |
| `snow_roof_slope` | `[3.4,1.41,2]`; pitch `1.67/2.6`, high at −Z |
| `snow_rail_top`, `snow_post_cap` | `[1.2,.075,.11]`, `[.12,.085,.12]`; additions to existing structural rails |
| `railing_snow_straight`, `railing_snow_corner`, `railing_snow_end` | `[1.8,1.125,.16]`, `[.3,1.125,.3]`, `[.3,1.135,.16]`; canonical porch modules plus top snow |
| `fence_post_snow` | `[.16,1.135,.16]`; original 1.05 m post plus cap |
| `icicle_short`, `icicle_medium`, `icicle_long` | `[.07,.18,.07]`, `[.10,.36,.10]`, `[.13,.65,.13]` |
| `icicle_cluster_mixed`, `icicle_cluster_sparse` | `[1.2,.65,.13]`, `[1.2,.36,.10]`; seven/three separated spikes |

The evergreen is exactly `outdoor:tree_03`: no trunk, branch, fringe, UV,
colour or atlas changes. Its current opaque needle lobes carry separate closed
snow coats on connected upward surfaces; bare undersides and selected whole
boughs remain visible. Both variants fit the original bounds. The
1284/1380-triangle review exception retains the canonical 708 triangles;
every other winter model is below 500. No original outdoor asset is replaced.

### Concept village construction (October 8)

`tools/props/parts/winter_village.py` adds seven static families, all placed in
Winter and the Model Zoo: coursed snow-capped stone walls and piers, a timber
lantern post, an open masonry entrance frame, a braced snowy door hood, deep
timber window frames and thin angular ice fragments. The entrance frame and
hood replace the old primitive front framing; operable leaves, doorway
clearances, steps and building footprints retain their completed behavior.
The old `snow_door_overhang` remains a reusable Zoo/library attachment.

The stone/wood/metal/amber fitted atlas has a committed 1024² master and 256²
native derivative. Four generated 1024² surface masters provide faceted snow,
fractured ice, slate masonry and weathered timber; deterministic periodic-edge
repair precedes model downsampling. `tools/props/build_winter_textures.py`
reproduces these derivatives from artwork files, without painting imagery.
The journal at `docs/style-upgrade-20261007/winter/` records the immutable
reference, individual audit, matching native cameras and review payloads.

All added snow and icicles are non-solid. Trees keep `[.6,6.8,.6]` trunk
colliders; boulders keep `[1.4,.85,1.3]`; perimeter rocks keep `[4,5,2.2]`.
Railing variants must use the original module's collider dimensions, including
swapping X/Z for quarter turns. The Winter Place keeps its three original
`guardrails[]` exactly, adding top/midrail and post-cap meshes. Full porch/fence
variants are reusable library assets; those overlays preserve the foundation
rail dimensions and collision.

### Materials and mounting

`winter:snow_01` uses the real 1024² seamless
`textures/floors/snow_01.png` at an 8 m repeat, matte with no emission or normal
perturbation. `winter:snow_packed_01` shares that artwork with a quieter cool
tint and 5 m repeat. `props/models/snow_surface.png` is its 256² native model
source. Imported variants retain their outdoor embedded PNG and use the
existing committed white-sheet path on their snow material. The static prop
uploader reserves a shared white binding instead of treating an absent texture
as the model’s first atlas; this keeps the original green/wood/rock art intact.

Origins are centred X/Z and base Y=0; +Z is front. Sink ground/cap bases 8 mm
into their support. A roof-slope cap covers native roof Z=−.9..+1.1 (leaving
the upper ridge and lower fascia exposed); its origin is native Z=+.1,
Y=.20846. Apply the same 1.4 scale/yaw as the cottage roof bay. Cap crowns and
edges vary without changing that pitch. Roof edges have a small 4 cm lip
beyond the supporting fascia. Icicle roots are all at catalogue height;
mount at `underside + .015 - height * scale`. **Prop Y is relative to the
local rendered floor**, including the raised deck and stepped tread heights.

The timber eave/awning boxes preserve their original colliders; their snow
placeholders are now real modular additions. Snow coverage varies around
shelters, entrances and exposed trees. Cleared paths, dry interiors, most
under-rail space, the forest spine and pond approaches remain usable.

### Remaining environment passes

Weather remains a separate pass. The northern-lights sky is described below;
warm lamps retain their existing lighting, and frozen water is described below.

### Northern-lights sky

`winter:tex_sky_aurora_01` loads the opaque RGB 2048×1024
`textures/sky/sky_aurora_01.png` through the existing sky background pass.
High uploads 2048×1024, Medium 1024×512 and Low 512×256. The sky-only decoder
limit preserves ordinary texture budgets. Distant green/cyan ribbons, small stars and restrained violet fringes
sit in an open deep navy sky. The sheet is 2:1 equirectangular: U wraps around yaw,
V=0 is the zenith, V=.5 is the horizon and V=1 is the nadir. Quiet blue wrap
margins and poles avoid obvious edge joins and pole pinching. No terrain is
painted into the sky; the map's real rock and tree silhouettes own the horizon.

The static sky uses brightness .85. Existing ambient .24 and the blue
`winter_moon` directional source (intensity .18) remain independent of the
artwork. Aurora colour never enters the lighting bake; warm local pools stay
amber against cool snow. The sky is neither fogged nor included in reflection
captures, following the existing renderer contract. No shaders, animation,
atmospheric rendering or dependencies were added. The built-in ImageGen output
was authored at 1774×887 with sharper stars/edges and fitted offline to the
2048×1024 POT contract (a slight enlargement, rather than the previous downsize); the shipped PNG is the
authoritative artwork. See `docs/reports/winter-aurora.md` for prompts and checks.

### String-light kit

`winter:string_lights_short`, `string_lights_medium`, `string_lights_long`
provide 2.8/4.0/6.6 m between attachment centres, .15/.28/.40 m sag and
3/5/7 amber bulbs. Bounds are `[2.85,.395,.09]`, `[4.05,.525,.09]`,
`[6.65,.645,.09]`; native bulb diameter is 9 cm and height 14 cm. The two
clips are at local X = ±span/2, Y = sag+.22. Base Y=0 is the lowest bulb
underside. Keep native scale; repeat spans on real supports instead of
stretching bulbs. `tools/props/string_lights.py` shares these anchors and
source offsets between the mesh builder and map author.

The opaque 128² sibling `string_lights.png` reuses the committed ball-light
atlas unchanged. Cable/socket materials remain dark; the glass material emits
amber at strength 1.0. Each bulb also owns one static point 2 cm below its
glass so it can light the environment without self-shadowing. Overhead lights
use .32 intensity, 4.5 m range; rails use .10, 3 m. No transparent glow cards
or new shaders are used. The standard compiler bakes all sources, and existing
emission bloom supplies a small optional halo.

Winter uses eight spans (38 string sources): two cottage fascias, one sheltered
lodge span, two porch rails, two square spans and one path crossing.
Six narrow existing timber posts support the gently angled square/path spans.
One low lamp marks the northern forest resting place; the surrounding forest,
pond perimeter and rear building faces remain cold. All cable spans are
non-solid; supports stand outside the packed route.

### Validation

```sh
python3 tools/props/build.py --check
python3 tools/assets/validate.py
python3 tools/assets/audit.py --workers 12 --out target/winter-assets/audit.json
python3 tools/textures/seam_repair.py --check assets/environment/winter/textures/floors/snow_01.png
python3 -m unittest tests.test_winter tests.test_winter_assets
cargo test --workspace --all-features
cargo run --release --bin places-compile -- build assets/levels/winter.json --workers 12
python3 tools/bench/capture_winter.py --root "$PWD" --quality high
```

The snow artwork was created with built-in imagegen: “square seamless opaque
snow albedo; pale near-neutral cool white, broad subtle polygonal blue-grey
variation, restrained noise, no baked lighting, tiny crystals, objects or
repeating motif.” Its generated source was resized to the surface/native
contracts and processed through the existing offline seam-repair tool.

## Integration anchors

- Square: `(0, 0)`, with lamps and stump seating; space for string lights.
- Lodge: bounds `[-16, -16]..[-7, -10]`, floor +0.6 m; side three-step flight,
  front ramp, sheltered deck, amber entrance lights, separate eave/awning edges.
- Cottages: `[8, -30]..[17, -24]` and `[-17, 4]..[-8, 10]`; real glazed windows,
  operable doors, furnishing and clear circulation.
- Pond: center `(11, -10)`; shallow tapered corner shelves and irregular banks, physically walkable
  ice floor, east railing, open west and south approaches.
- Forest spine: `(0, 16)..(0, -36)`; unobstructed long weather-test sightline.
- Perimeter: visible five-metre rock escarpments, not invisible walls.

The foundation builder authors geometry and references only. The normal Places
compiler prepares lightmaps, navigation, resource identities and packages.

### Integration and shelter

The October 7 integration pass keeps the lodge deck, covered rail tops and
covered window ledges dry. The porch-edge cap remains a reusable library asset
and Model Zoo exhibit, rather than requiring a display underneath a roof.
Icicle groups flank the lodge doorway at varied lengths, with a short centre
drip. Cottage banks vary in scale; shoreline drifts sit on the shallow dry
shelves, clear of the west ice crossing. Timber surrounds and mullions frame
the real glazed openings. Concrete skirts close the raised lodge's exposed
foundation without changing its movement surfaces or colliders.

Forest spacing and tree scale vary more widely, with intentional grove gaps
and 61 evergreen placements. The main spine and cottage routes stay clear.
The source generator also synchronizes the severe review scene while retaining
its exact weather settings; the frozen Prompt 7 evidence is unchanged.

## Frozen water

`winter:ice_01` uses the real seamless 1024² `textures/floors/ice_01.png` at
an 8 m repeat: blue-gray cloudy facets, sparse cracks and pale highlights. It
blends at 0.84 opacity with modest broad sheen, no mirror/reflection image, no
normal map or emission. The opaque `winter:ice_depth_01` reuses the sheet
under the pond. The solid floor uses material `ground_surface: "ice"`, which
applies reduced friction and gradual steering only on the current support.

To freeze a winter pond, substitute an ice floor region at its waterline for
the liquid volume. `swimming: false` alone never provides collision. Winter
keeps its -0.16 m shallow shoreline step and adds supported dry-shore drifts.

Ice albedo was generated with the built-in imagegen tool, then resized to the
existing 1024² floor-sheet contract and seam-repaired with the repository tool.
Prompt: “square seamless top-down opaque retro low-poly frozen pond albedo;
quiet blue-gray ice, subtle cloudy polygonal variation, sparse angular hairline
cracks and pale highlights, even neutral illumination; no scene, snow, objects,
text, photographic detail, mirror reflection or baked shadows.”
