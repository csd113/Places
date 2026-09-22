# Office environment

The initial office collection: the classic liminal materials, the fluorescent
ceiling fixture and clearly office-oriented furniture.

Every surface material is a **data definition** that names an external PNG
texture (Goal 4.5). The material owns the tiling/tint; the texture owns the
image file, so a PNG can be replaced without touching the catalog, and the
catalog entry can move without touching a level.

| material | type | texture | PNG |
| --- | --- | --- | --- |
| `core:wallpaper_yellow_01` | material (wall) | `core:tex_wallpaper_yellow_01` | `textures/walls/wallpaper_yellow_01.png` |
| `core:wallpaper_stained_01` | material (wall, damaged) | `core:tex_wallpaper_stained_01` | `textures/walls/wallpaper_stained_01.png` |
| `core:carpet_beige_01` | material (floor) | `core:tex_carpet_beige_01` | `textures/floors/carpet_beige_01.png` |
| `core:carpet_damp_01` | material (floor, damaged) | `core:tex_carpet_damp_01` | `textures/floors/carpet_damp_01.png` |
| `core:ceiling_panel_01` | material (ceiling) | `core:tex_ceiling_panel_01` | `textures/ceilings/ceiling_panel_01.png` |
| `core:ceiling_stained_01` | material (ceiling, damaged) | `core:tex_ceiling_stained_01` | `textures/ceilings/ceiling_stained_01.png` |
| `core:fluorescent_panel_01` | light | — (untextured fixture) | — |
| `core:desk`, `core:chair`, `core:cabinet`, `core:water_cooler`, `core:vending_machine` | prop | embedded in each GLB | `props/models/*.glb` |

The six PNGs are deliberately **seed** art: they preserve the pre-4.5 look and
prove the pipeline. Goal 5 replaces them with the real office artwork. Replace
or edit any of them and the next launch shows the new pixels; see
`../../README.md` for the texture tooling and the workflow.

The office carpet carries the metre checker **in the PNG** (four 64 px
quadrants of a 128 px two-metre tile). There is no renderer-side checker bake
any more.

Generic props (couch, bed, plants, utilities, ...) are deliberately **not**
listed here: they belong to no theme and live under `../../core/`.
