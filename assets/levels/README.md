# Shipped levels

`places_demo.json` is the only level bundled with the game. It is discovered at
startup, appears in the Level Select menu, and is the level the game boots into.
Boot straight into it with `LIMINAL_LEVEL=places_demo`.

`assets/levels/` is the shipped-level directory: every `*.json` here is
packaged. Nothing here is hard-coded, so adding a file adds a menu entry and
removing one removes it. Drop-in user levels live in `levels/` at the repository
root and are discovered alongside the demo without being packaged;
`tests/fixtures/levels/` holds the engine regression fixtures and is never
packaged.

## The official demo

| id | name | what it is |
| --- | --- | --- |
| `places_demo` | Places Demo | **The showcase.** One continuous route through everything the project does: office → doorways and windows → red stair hall → empty pool → two steps up → quiet corridor → final doorway into the unmade world. Start here. |

## Regression fixtures

The fixtures live in `tests/fixtures/levels/`, outside the shipped content, so
they do not appear in a packaged game's Level Select menu. They are
load-bearing: each one is named by an automated test or a documented manual
check.

| id | name | what depends on it |
| --- | --- | --- |
| `test_room` | Test Room | The minimal single-room sample with a door and a window, used by the loader, renderer and level-editor round-trip tests. |
| `prop_showcase` | Prop Showcase (dev) | `test_showcase_level_collision_matches_the_solid_flags` and `the_showcase_level_renders_every_core_prop_with_real_geometry`. |
| `prop_stress` | Prop Stress Test (dev) | `the_stress_level_batches_repeats_into_one_draw_per_model_and_cell` — ~150 repeated placements across nine models. |
| `pool_showcase` | Pool Showcase | The Pool-family geometry and collision checks in `src/render/tests.rs` and `tests/test_package.py`. |
| `vertical_diagnostic` | Vertical Diagnostic | The vertical-geometry fixtures: an elevated room reached by a region staircase, a walkable recess and a blocked deep recess, a gable room with eave and ridge fixtures, RGB-lit corners, decals. Used by the loader and renderer geometry tests. |
| `rendering_diagnostic` | Rendering Diagnostic | The decal-sheet coverage test (all eight sheets on one level) and the stain-overlay test. |
| `lighting_isolation` | Lighting Isolation | The whole `src/lighting_isolation.rs` suite (13 cells, one per wall-boundary rule: blocked white light, blocked colour, doorway transmission, window sill and header, two coloured rooms, dark neighbour, interior partition, lit corners, unlit control). The acceptance fixture for the Goal 5.5 lighting work. |
| `lighting_diagnostic` | Lighting Diagnostic | `emitted_wall_faces_are_lit_by_the_room_they_open_into` — a wall authored across a room boundary. |

`prop_showcase` and `prop_stress` are **generated**: running
`python3 tools/levels/build_fixture_levels.py` overwrites them. Change the
generator, not the JSON. Every other fixture is hand-authored and safe to edit
directly.

## What used to ship here

The original level list was audited down to the official demo: `level_1`, the
three residential levels, the Office and Pool showcases, the texture diagnostic
and the two generated asset demos (`asset_demo`, `asset_maintained`) were
removed from the game's content, along with the tests that only asserted those
levels' own design. The demo, and any user level, still resolve the complete
catalog, so no asset was removed: every model, texture and material in
`assets/catalog.json` remains available to `Places Demo`, the engine and
user-created levels.
