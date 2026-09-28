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

The diagnostic textures are deliberately loud and asymmetric
(diagonal stripes with up arrows, quadrant markers, chequerboards): a capture
proves surface assignment, orientation, tiling and replacement at a glance.
They are architecture-test assets, not art: the Office and Pool content is
separate shipped artwork.

`diagnostic_alt_01` is intentionally **not** a power of two, and
`diagnostic_alpha_01` carries a transparent margin and a translucent ring, so
the loading tests have an NPOT and an RGBA case in the shipped set.

The diagnostic **levels** are engine regression fixtures and live in
`tests/fixtures/levels/` (`lighting_diagnostic.json`,
`rendering_diagnostic.json`, `vertical_diagnostic.json`), outside the shipped
content, because levels are discovered from the level directories, not the asset
catalog. New diagnostic models, textures or fixtures belong in this
directory.

* `tests/fixtures/levels/vertical_diagnostic.json` is the
  vertical-geometry fixture: a normal room at floor `0` with the default 4.0 m
  ceiling, a staircase built from floor regions that climbs to an elevated room
  at `floor_y: 2.0`, a room with a walkable recess and a blocked deep recess, a
  gable room with eave and ridge fixtures, RGB-lit corners and decals.
* The diagnostic materials and textures stay catalogued and resolvable like any
  other catalogued asset, so tests and user levels can reference them. The
  renderer loads `core:tex_diagnostic_alt_01` directly in a test that proves
  the non-square, non-power-of-two path; the remaining diagnostic textures are
  available for the same kind of checks.

The RGB-lighting, decal, external-asset and vertical-geometry diagnostic
content loads and resolves through the catalog.
