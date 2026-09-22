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

## Goal 5 artwork

The six PNGs are the **final Office artwork** (Goal 5), not the Goal 4.5 seed
set. They are 128x128 8-bit RGBA, opaque, and tileable in both directions
(every pattern period divides the sheet and every noise field wraps). They stay
pale and near-neutral, because the material `tint` and the baked lighting
multiply into the sampled texel; the carpet is the deliberate exception (its
material has no tint), so it is painted at the historical warm-brown albedo.

* `wallpaper_yellow_01` — pale-printed stock: fine vertical striation, a
  pinstripe pair and a half-drop dot motif on a 25 cm cell, plus a low-frequency
  patina. Repetitive and commercial rather than ornate.
* `wallpaper_stained_01` — the same paper with restrained water damage: soft
  damp fields and a few vertical runs. No dark outlines, so the damage does not
  turn into a repeating pattern.
* `carpet_beige_01` — short-pile carpet: low-frequency mottle, fine directional
  fibre and 2 px pile loops. **There is no metre checker anywhere**: the
  historical 1 m bright/dark quadrants were removed in Goal 5, and the sheet is
  painted so Level 1's floor keeps its previous brightness and warmth.
* `carpet_damp_01` — the same pile, darker, cooler and slightly flattened in
  soft damp patches.
* `ceiling_panel_01` — a 2x2 grid of 1 m suspended acoustic panels (2 px T-bar
  plus a 1 px shadow groove), slightly yellowed, with per-panel tone variation
  and pinhole speckle.
* `ceiling_stained_01` — the same grid with a believable water tide mark on one
  panel and a smaller leak on another, clipped by a grid fade so the T-bar
  still reads through the damage.

`tools/textures/office_art.py` is the deterministic, stdlib-only regeneration
path; the shipped PNGs are authoritative and hand-painted replacements are
equally valid. `tools/textures/build.py --check` gates the budget.

The official demo `../../levels/places_demo.json` exercises the set: a warm
office reception and workroom on the yellow wallpaper and panel ceiling, the
stained/damp variants in the areas the building has given up on, sparse desks
and task chairs, cabinets, a water cooler and floor decals.

Generic props (couch, bed, plants, utilities, ...) are deliberately **not**
listed here: they belong to no theme and live under `../../core/`.
