# Home environment

The Home content pack: the warm residential set the Places Demo's route ends
in, across the living area and the balcony. Materials are domestic — off-white
paint and wallpaper, hardwood, cream carpet and tile, painted ceilings, white
and sauna-wood doors — and the props are the practical fittings of an occupied
flat: base and wall cabinets, a CRT television, a table plant, a ball light, a
wall switch and a tableware set.

Like every theme, Home is an organizational collection rather than a placement
rule: any asset may be used in any level (see
[`../../README.md`](../../README.md)).

## Surface textures

| texture id | file | what it is |
| --- | --- | --- |
| `home:tex_wallpaper_offwhite_01` | `textures/walls/wallpaper_offwhite_01.png` | off-white wallpaper |
| `home:tex_wallpaper_pattern_01` | `textures/walls/wallpaper_pattern_01.png` | patterned wallpaper |
| `home:tex_wall_paint_offwhite_01` | `textures/walls/wall_paint_offwhite_01.png` | off-white wall paint |
| `home:tex_baseboard_white_01` | `textures/walls/baseboard_white_01.png` | painted white skirting |
| `home:tex_baseboard_wood_01` | `textures/walls/baseboard_wood_01.png` | wood skirting |
| `home:tex_handrail_wood_01` | `textures/walls/handrail_wood_01.png` | wooden handrail |
| `home:tex_hardwood_oak_01` | `textures/floors/hardwood_oak_01.png` | oak floor boards |
| `home:tex_hardwood_walnut_02` | `textures/floors/hardwood_walnut_02.png` | walnut floor boards |
| `home:tex_carpet_cream_01` | `textures/floors/carpet_cream_01.png` | cream carpet |
| `home:tex_tile_home_01` | `textures/floors/tile_home_01.png` | interior floor tile |
| `home:tex_threshold_wood_01` | `textures/floors/threshold_wood_01.png` | wooden door threshold |
| `home:tex_ceiling_white_01` | `textures/ceilings/ceiling_white_01.png` | painted ceiling |
| `home:tex_ceiling_plaster_01` | `textures/ceilings/ceiling_plaster_01.png` | plaster ceiling |
| `home:tex_door_white_01` | `textures/doors/door_white_01.png` | satin white door leaf |
| `home:tex_sauna_wood_01` | `textures/doors/sauna_wood_01.png` | light cedar boards for the sauna |
| `home:ceiling_light_round` | `textures/lights/ceiling_light_round_01.png` | round flush-mount diffuser (a catalog `light` fixture face) |

The `home:door_white_01` / `home:sauna_wood_01` materials are the two door
surfaces the demo's doors are built from; their frames share
`home:baseboard_white_01` / `home:baseboard_wood_01` with the skirting.

## Props

| prop id | model | what it is |
| --- | --- | --- |
| `home:cabinet_base` | `props/models/cabinet_base.glb` | off-white shaker base cabinet with a laminate countertop |
| `home:cabinet_wall` | `props/models/cabinet_wall.glb` | wall cabinet with two doors |
| `home:crt_tv` | `props/models/crt_tv.glb` | boxy CRT television |
| `home:ball_light` | `props/models/ball_light.glb` | hanging ball light |
| `home:wall_switch` | `props/models/wall_switch.glb` | light switch plate |
| `home:plant_table` | `props/models/plant_table.glb` | potted table plant |
| `home:knife` | `props/models/knife.glb` | table knife |
| `home:fork` | `props/models/fork.glb` | table fork |
| `home:spoon` | `props/models/spoon.glb` | table spoon |
| `home:plate` | `props/models/plate.glb` | dinner plate |
| `home:bowl` | `props/models/bowl.glb` | bowl |

See [`../README.md`](../README.md) for how the environment themes are
organized, and [`../../README.md`](../../README.md) for the catalog format and
resolution flow.

## Hanging painting

The demo's west wall at `(53.33, 10.0)` displays **The Temptation of Adam and
Eve**. Aim at the frame and press E to toggle its title. This uses two reusable
core assets: `core:painting_frame_landscape_01` for the timber frame and
`core:decal_temptation_adam_eve_01` for the artwork. The supplied photograph's
full 900×546 pixels are preserved inside a transparent 1024×1024 PNG; High
quality displays it without downscaling. The painting is 1.800×1.092 m, with
a 50 mm timber border. See [the asset specification](../../../docs/ASSET_SPECIFICATION.md#72-framed-hanging-paintings)
for the paired placement contract.

## October 2026 concept reconstruction

The immutable sheets guide cream upholstery, Shaker cabinetry, narrow warm
timber boards and domestic ceramic. The playable demo now uses local Home
sofa/armchair, coffee/dining tables, slat chairs, console, populated shelf,
bound rug, linen lamp, sink and cooker. New mug, books, cushions, kettle,
toaster, outlets, framed landscapes, cabinet strips and profiled casing are
built and placed. Existing core families remain available unchanged.

Four local surface variants (`wall_paint_warm_01`, `hardwood_oak_warm_01`,
`ceiling_warm_01`, `ceiling_plaster_warm_01`) preserve shared originals used
elsewhere. `backsplash_01` adds 15 cm ceramic in a 1.2 m repeat. Floor tile
is 30 cm; wallpaper has restrained sage sprigs. New/rebuilt prop imagery
keeps 1024² masters and fitted 256² native PNGs, embedded in the GLBs.

See the [matched native comparison and complete coverage inventory](../../../docs/style-upgrade-20261007/home/README.md)
and [asset contracts](../../../docs/ASSET_SPECIFICATION.md#89-home-concept-furniture-and-static-domestic-fittings).
`tools/levels/refine_home.py` reapplies tagged demo dressing; it preserves the
existing routes, interactions, stairs and loft. The reference's outdoor
balcony/sliding glazing remains a documented architectural omission.
