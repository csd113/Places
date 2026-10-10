# Beach asset kit

Phase 2 reconstructs the immutable Tropical Beach board in the existing asset
formats. It supplies construction pieces for Phase 3; the finished connected
`beach_demo` is not part of this phase. All IDs use `beach:`. Models use metres,
+Y up, +Z forward, horizontally centred base origins, opaque PNG albedo and
the existing flat geometric-normal path. No runtime texture generation or new
lighting, water, HDR storage or NPC system is introduced.

| Concept | Registered resources |
| --- | --- |
| B01 | `sand_patch`, `grassy_bank`, `shoreline_foam`; sand/grass/foam surfaces and real shore support |
| B02 | Polygon-cell `tex_water_01`, `water_shallow_01`, `water_deep_01`; existing WaterDef volumes |
| B03 | `coastal_rock`, `coastal_rock_wide`, `island`, open `sea_arch` |
| B04 | Tapered `lighthouse`, gallery, fitted windows and red conical cap |
| B05 | `palm`, `palm_small`: bent banded trunks and ten pointed folded fronds |
| B06–B09 | X-braced `crate`, square-post `dock`, open yellow-hip-roof `kiosk`, `stilt_house_blue`, `hut_brown` |
| B10–B12 | Coral/ivory `umbrella`, blue timber `lounge_chair`, right-arrow `signpost` |
| B13 | `town_house_cream`, `town_house_blue`, pale `town_house_coral`, `town_arch`, `town_stairs`, `town_terrace`, `town_parapet` |
| B14 | `town_shrub`, palms, orange/blue `bunting` |
| B15 | `beach:seagull`: `idle` / `fly` |
| B16 | `beach:crab`: `idle` / `walk` |
| B17 | `beach:fish`: `swim` |
| B18 | Seamless `tex_sky_day_01`: pale horizon, faceted clouds, small sun |

There are 27 static GLBs and three independently skinned animal GLBs, with
five looping clips. Static geometry totals 6,348 triangles; animals add 1,438.
The largest static export is a 782-triangle palm. Registration adds 11 texture
resources and 11 material definitions. All pre-existing catalog entries remain
unchanged. The Model Zoo uses a compact annex through its existing east exit;
existing displays, routes and lighting values retain their original placement.

## Texture contracts and sources

Ten opaque seamless surface families retain square 1024-pixel masters beside
their 512-pixel runtime PNGs. Seven are the board's labelled sand, water, rock,
stucco, wood plank, palm trunk and palm leaf; grass, yellow roof and foam supply
the other depicted surfaces. Broad cells and marks survive the existing Low
texture budget. Water coverage comes from the material/volume BLEND contract,
not PNG alpha. The sky is 2048×1024, wraps U exactly, and has no baked islands.
Its sun is at bearing +45°, elevation +45°; `daylight()` supplies the matching
directional light and cool ambient fill.

The static atlas is a 1024² master and fitted 256² Lanczos derivative under
`props/models/`. Its fixed 4×4 rows are:

```
sand       grass      rock       stucco
wood       plank      palmtrunk  palmleaf
blue       coral      yellow     ivory
dark       turquoise  window     foam
```

Umbrella sectors and chair sling use the coral/ivory/blue cells. The pale town
coral uses an explicit model vertex tint over ivory, retaining the shared atlas.
Each animal owns a 1024² source and 256² fitted derivative under
`assets/entities/beach_<animal>/textures/`; its GLB embeds that exact native PNG.
Animal READMEs describe their atlas, rig, measured poses and initialization.

Normal builders load committed PNGs. Intentional offline painting is separate:

```sh
python3 tools/textures/beach_art.py --check
python3 tools/props/build.py --check
python3 tools/entities/build_beach_seagull.py --check
python3 tools/entities/build_beach_crab.py --check
python3 tools/entities/build_beach_fish.py --check
python3 tools/entities/validate_entities.py --workers 12
python3 tools/entities/check_clip_boundaries.py
```

Static deterministic geometry sources are `tools/props/parts/beach_nature.py`
and `beach_structures.py`; animal sources are `tools/entities/build_beach_*.py`.
`tools/textures/beach_art.py --author` deliberately reauthors the static PNGs.
An animal's `--author-textures` deliberately reauthors its own PNGs. Export the
corresponding GLBs after changing any source image.

## Supported authoring pieces

`tools/levels/beach_components.py` returns existing v3 arrays. `shore_segment`
provides a dry/wade/swim strip with a real lowered sea floor, two gentle ramp
runs and shallow/deep water volumes. Choose adjoining strip endpoints to follow
a curved coast; decorative sand/foam meshes do not create walking support.
Its containing room begins at the seabed, so submerged entity anchors receive
prepared lighting; raised regions provide dry sand at world Y=0. The ramp
strips deliberately keep their run along the engine's longer axis.
Water X edges sit 2 cm inside each strip, clearing the engine's 1 cm edge
tolerance and preserving room ownership when adjoining strips have different shore positions. Apply
the same small inset to water placed in separately lowered room basins.
Place dock regions outside those ramps so support has one owner at each point.

`beach_structures.placed_components(name, x, z, base_y=0, floor_y=0,
rotation_degrees=0, identity=None, floor_at=None)` returns the visual instance
and separate real `props`, `floor_regions`, `stairs`, `archways` arrays. Merge
them into a containing room. `floor_y` is the containing room's absolute floor;
`base_y` is the requested world base. For existing lowered regions or ramps,
pass `floor_at(x,z)` returning their world floor height. Quarter-turn rotations
are supported. Use a unique `identity` for each module.

The dock has deck support and individual post/beam blockers; hollow huts keep
wall/post blockers separate from their doors. Town portals have separate tight
piers and curved-header blockers; 12 stair tread regions provide real support
after rotations. Companion support faces sit 5–6 mm within the visible stock
to avoid coplanar overlays.
Tiny existing `outdoor:collision_peg` carriers sit inside that stock, with
`occludes:false`; their explicit collision size is compensated for their scale.
The launcher model's catalog envelope never substitutes for an open interior.

`beach_nature.placed_components("sea_arch", ...)` supplies the open visual and
separate tight pier/header blockers, with the same world-base, floor callback
and quarter-turn contract. The caller owns its supported approach or seabed.
Other nature models remain decorative. Author rocks/islands with
separate blockers and floors where reachable. Palm collision follows the narrow
bent trunk, not the canopy. Exact trunk bounds are documented in the generator.
Use `occludes:false` for thin foam and animated animals where appropriate.

Animal `y` is floor-relative. A perched gull uses `y:0` on an actual deck;
airborne gulls remain near their authored anchor; submerged fish need a positive
offset above their lowered floor. Start each intended clip with an explicit
`play_animation` action; `animation.playing` alone does not start the renderer.
Crab walking is in place, with measured reference speed about 0.160625 m/s.
Existing short routes can translate it; no complex patrol system is needed.
