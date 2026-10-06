# Winter asset set

The Winter foundation is `assets/levels/winter.json`, authored by
`tools/levels/build_winter.py`, and shipped as `winter.placesmap`. The `winter`
catalogue theme and `winter:*` material IDs use the ordinary asset registry;
there is no separate loader or runtime image generation.

Read `docs/ASSET_SPECIFICATION.md` and `docs/MAP_AUTHORING_GUIDE.md` before
introducing winter art. Future physical resources belong under this directory:

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

## Evergreen foundation

`outdoor:tree_03` is the canonical evergreen: **3.2 × 6.8 × 3.2 m**.
Its source is `tools/props/parts/outdoor_kit.py::build_tree_03`; its committed
resource is `environment/outdoor/props/models/tree_03.glb` with the sibling
PNG. The trunk, six irregular branch tiers and masked foliage fringe cards
remain unchanged. Winter uses that same model with a narrow trunk collider
`[0.6, 6.8, 0.6]`, scaled with the prop, and `occludes: false` to avoid treating
the transparent canopy as a solid light-blocking box.

The next pass should add `winter:tree_snow_01` as a **separate** GLB/PNG asset,
reusing the original evergreen builder, dimensional foundation, foliage and
UV contract. Add modeled snow on upward-facing tiers, preserving dark foliage
and the shared original. No alias or fake snow model is registered in this pass.

## Temporary foundation materials

| ID | Shared real PNG | Later replacement |
| --- | --- | --- |
| `winter:snow_01` | `home:tex_ceiling_plaster_01` | Dedicated tileable snow; currently ground/banks/eaves/awning |
| `winter:snow_packed_01` | `outdoor:tex_concrete_pavement_01` | Packed-snow path/tread artwork |
| `winter:ice_01` | `core:tex_glass_clear_01` | Stylized ice artwork and surface-specific friction |

These are material definitions, not duplicate images. Ice is deliberately
opaque and uses ordinary floor collision at −0.16 m, with no liquid volume,
transparency or slipping behavior yet. Snow on conifers, boulders and rails,
modular drifts, proper snow roof caps, icicles, string lights, aurora and weather
are subsequent passes. Existing stars and warm exterior lamps provide the
initial nighttime presentation. Roofs use the existing shingle/gable kit with pale snow eaves and awning;
full accumulated snow caps are deferred. Ceiling undersides use shared white
plaster; the terrace deck and enclosed cottage floors stay dry wood.

## Integration anchors

- Square: `(0, 0)`, with lamps and stump seating; space for string lights.
- Lodge: bounds `[-16, -16]..[-7, -10]`, floor +0.6 m; side three-step flight,
  front ramp, sheltered deck, amber entrance lights, separate eave/awning edges.
- Cottages: `[8, -30]..[17, -24]` and `[-17, 4]..[-8, 10]`; real glazed windows,
  operable doors, furnishing and clear circulation.
- Pond: center `(11, -10)`; clipped rectangular shoreline, physically walkable
  ice floor, east railing, open west and south approaches.
- Forest spine: `(0, 16)..(0, -36)`; unobstructed long weather-test sightline.
- Perimeter: visible five-metre rock escarpments, not invisible walls.

The foundation builder authors geometry and references only. The normal Places
compiler prepares lightmaps, navigation, resource identities and packages.
