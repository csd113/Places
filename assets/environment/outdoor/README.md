# Outdoor environment

The Outdoor content pack: muted night lawns, dirt and gravel paths, concrete
walkways, modular houses, vegetation and warm exterior lamps. The collection
also includes road, sidewalk and landscape props for the Lantern Hollow showcase.

Like every theme, Outdoor is an organizational collection rather than a placement
rule: any asset may be used in any level. Catalog entries use the `outdoor:`
namespace and are defined in [`../../catalog.json`](../../catalog.json).

## Surface textures

| texture id | file | what it is |
| --- | --- | --- |
| `outdoor:tex_grass_ground_01` | `textures/ground/grass_ground_01.png` | muted grass ground |
| `outdoor:tex_dirt_gravel_01` | `textures/ground/dirt_gravel_01.png` | packed dirt with scattered gravel |
| `outdoor:tex_concrete_pavement_01` | `textures/ground/concrete_pavement_01.png` | concrete walkway |
| `outdoor:tex_house_siding_01` | `textures/house/siding_01.png` | horizontal painted siding |
| `outdoor:tex_house_roof_shingle_01` | `textures/house/roof_shingle_01.png` | dark asphalt shingles |
| `outdoor:tex_sky_stars_01` | `textures/sky/sky_stars_01.png` | faint-star night sky |

The ground and house textures tile in both axes. The sky is a 2:1
equirectangular sheet. Surface materials reference these PNGs and define their
tiling and appearance in the catalog.

## Path decals

| decal id | file | what it is |
| --- | --- | --- |
| `outdoor:decal_path_edge_01` | `textures/decals/path_edge_01.png` | straight feathered path edge |
| `outdoor:decal_path_end_01` | `textures/decals/path_end_01.png` | rounded feathered path end |
| `outdoor:decal_path_corner_01` | `textures/decals/path_corner_01.png` | soft path bend or junction |

These PNGs use blended alpha to soften the transition between dirt and grass.

## Props

Models and their accompanying PNG artwork live in `props/models/`.

| family | catalog ids | what it contains |
| --- | --- | --- |
| Vegetation | `outdoor:grass_patch_small`, `outdoor:grass_patch_large`, `outdoor:tree_01`, `outdoor:tree_02`, `outdoor:tree_03` | grass tufts and trees |
| Exterior lighting | `outdoor:lamp_stand`, `outdoor:lamp_fence`, `outdoor:lamp_wall`, `outdoor:streetlight` | garden, fence, eave and street lamps |
| House modules | `outdoor:house_wall_*`, `outdoor:house_roof_*`, `outdoor:house_corner_trim`, `outdoor:house_01_*` through `outdoor:house_05_*` | walls, windows, doorways, roofs, trim and themed porch parts |
| Porch and fence fittings | `outdoor:porch_post`, `outdoor:porch_railing_*`, `outdoor:fence_post`, `outdoor:concrete_step` | posts, railings and steps |
| Showcase modules | `outdoor:showcase_*` | roads, markings, sidewalks, curbs, rocks, a campfire, a road gate and a stump seat |

Consult the catalog for each prop's size, collision defaults and placement notes.
Emissive lamp and campfire artwork needs an authored light to illuminate the
surrounding scene.

See [`../README.md`](../README.md) for theme organization,
[`../../README.md`](../../README.md) for catalog resolution, and
[`../../../docs/MAP_AUTHORING_GUIDE.md`](../../../docs/MAP_AUTHORING_GUIDE.md)
and [`../../../docs/ASSET_SPECIFICATION.md`](../../../docs/ASSET_SPECIFICATION.md)
for authoring requirements.

## October 2026 concept reconstruction

The [Outdoors visual journal](../../../docs/style-upgrade-20261007/outdoors/README.md)
records immutable reference analysis, static-object coverage, native matched
views and validation. Trees, lanterns and rocks have closed deliberate shapes;
the base/family-02 facade stock has fitted warm artwork and emissive windows.
Eight new static families fill the reference's missing shrub/fence/porch/geology
layers and provide a seating fire without animation or character ownership.
Masters stay beside 256px native prop and 512px ground derivatives. Normal
builders load committed PNGs; `author_outdoors.py` is an explicit offline
authoring tool. Existing sky, encounters and all Winter assets remain intact.
