# Diagnostic assets

Development and validation content that exists to test the renderer, not to
furnish a level. Diagnostic assets are `asset_class: diagnostic` and keep that
class rather than being labelled Office or Pool just to fit the theme system.

Currently catalogued here:

| asset | type | resource |
| --- | --- | --- |
| `core:decal_test_01` | decal | generated (internal validation marking) |
| `core:diagnostic_wall_01` | material (wall) | `core:tex_diagnostic_wall_01` → `textures/diagnostic_wall_01.png` |
| `core:diagnostic_floor_01` | material (floor) | `core:tex_diagnostic_floor_01` → `textures/diagnostic_floor_01.png` |
| `core:diagnostic_ceiling_01` | material (ceiling) | `core:tex_diagnostic_ceiling_01` → `textures/diagnostic_ceiling_01.png` |
| `core:diagnostic_alt_01` | material (floor) | `core:tex_diagnostic_alt_01` → `textures/diagnostic_alt_01.png` (96×64 NPOT) |
| `core:diagnostic_alpha_01` | material (wall) | `core:tex_diagnostic_alpha_01` → `textures/diagnostic_alpha_01.png` (RGBA) |

The Goal 4.5 diagnostic textures are deliberately loud and asymmetric
(diagonal stripes with up arrows, quadrant markers, chequerboards): a capture
proves surface assignment, orientation, tiling and replacement at a glance.
They are architecture-test assets, not art: Goal 5 replaces the Office and Pool
content, not these.

`diagnostic_alt_01` is intentionally **not** a power of two, and
`diagnostic_alpha_01` carries a transparent margin and a translucent ring, so
the loading tests have an NPOT and an RGBA case in the shipped set.

The diagnostic **levels** themselves live with the rest of the shipped levels
(`../levels/lighting_diagnostic.json`, `../levels/rendering_diagnostic.json`,
`../levels/vertical_diagnostic.json`, `../levels/texture_diagnostic.json`)
because levels are discovered from the level directories, not the asset
catalog. Future diagnostic models, textures or fixtures belong in this
directory.

* `../levels/vertical_diagnostic.json` is the Goal 4 vertical-geometry level: a
  normal room at floor `0` with the default 4.0 m ceiling, a staircase built from
  floor regions that climbs to an elevated room at `floor_y: 2.0`, a room with a
  walkable recess and a blocked deep recess, a gable room with eave and ridge
  fixtures, RGB-lit corners and decals.
* `../levels/texture_diagnostic.json` is the Goal 4.5 external-texture level:
  diagnostic materials on all three surface families, an NPOT texture, an RGBA
  sheet, a coalesced material-run overlay, decals, a gable, a walkable recess, a
  region staircase into an elevated room, and warm/blue/white fixtures for the
  lighting check. Boot it with `LIMINAL_LEVEL=texture_diagnostic`.

Goal 1 (RGB lighting), Goal 2 (stable overlays/decals), Goal 3 (assets) and
Goal 4 (vertical geometry) diagnostic content still loads and resolves
unchanged.
