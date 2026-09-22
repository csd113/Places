# Shipped levels

Every file here is discovered at startup and appears in the Level Select menu,
alongside the drop-in levels in `levels/` at the repository root. Nothing here
is hard-coded: adding a JSON file adds a menu entry, and removing one removes
it. `tools/assets/validate.py` and `tests/test_package.py` validate every level
in both directories.

Boot straight into one with `LIMINAL_LEVEL=<id> cargo run`. The id is the
level's own `id` field (which is also the file name for everything except
`level1.json`, whose id is `level_1`).

## The official demo

| id | name | what it is |
| --- | --- | --- |
| `places_demo` | Places Demo | **The showcase.** One continuous route through everything the project does: office → doorways and windows → red stair hall → empty pool → two steps up → quiet corridor → final doorway into the unmade world. Start here. |

## Environment showcases

| id | name | what it is |
| --- | --- | --- |
| `office_showcase` | Office Showcase | The Office family on its own: a small institutional suite, warm fluorescent fixtures, sparse desks and chairs, one room on the damaged material set, floor decals. |
| `pool_showcase` | Pool Showcase | The Pool family on its own: a recessed empty basin and walk-in shelf (`floor_regions`), ladder, guardrails with collision, patio set, curtains, both Pool light fixtures and the external NO DIVING sign. |

## Residential levels

| id | name | what it is |
| --- | --- | --- |
| `the_residence` | The Residence | One very large house: entrance hall, living rooms, kitchen wing, bedroom corridors, service rooms, and a back wing that has been leaking for years. |
| `quiet_apartments` | Quiet Apartments | An apartment building whose corridors and interiors run into one another; the far apartments are only reachable through their neighbours. |
| `after_the_leak` | After the Leak | A house with a long-standing water problem spreading out of its service core. |
| `level_1` | Level 1 | The original residential grid. It is also embedded in the binary (via `include_str!`) as the fallback level, so it must stay at `assets/levels/level1.json`. |

The three named residential levels share one authored idea: they are maintained
near the spawn and decay as you walk, room by room, so the far end is darker,
damp and stained. The progression is static, baked into the level's materials
and fixture brightnesses — nothing changes while you play.

## Development and regression fixtures

These are load-bearing. Each one is named by an automated test, a bench script
or a documented manual check, and deleting it turns a test red or removes the
only coverage of a case that a showcase does not reach.

| id | name | what depends on it |
| --- | --- | --- |
| `lighting_isolation` | Lighting Isolation | The whole `src/lighting_isolation.rs` suite (13 cells, one per wall-boundary rule: blocked white light, blocked colour, doorway transmission, window sill and header, two coloured rooms, dark neighbour, interior partition, lit corners, unlit control). The acceptance fixture for the Goal 5.5 lighting work. |
| `lighting_diagnostic` | Lighting Diagnostic | `emitted_wall_faces_are_lit_by_the_room_they_open_into` — a wall authored across a room boundary. |
| `vertical_diagnostic` | Vertical Diagnostic | The vertical-geometry fixtures: an elevated room reached by a region staircase, a walkable recess and a blocked deep recess, a gable room with eave and ridge fixtures, RGB-lit corners, decals. Used by the loader and renderer geometry tests. |
| `rendering_diagnostic` | Rendering Diagnostic | The decal-sheet coverage test (all eight sheets on one level) and the stain-overlay test. |
| `texture_diagnostic` | Texture Diagnostic | The external-texture cases with no other home: the diagnostic material family, the deliberate 96×64 non-power-of-two texture, an RGBA alpha sheet and a coalesced material-run overlay. Manual; no automated test loads it. |
| `test_room` | Test Room | The minimal single-room sample with a door and a window, used by the loader, renderer and level-editor round-trip tests. |
| `prop_showcase` | Prop Showcase (dev) | `test_showcase_level_collision_matches_the_solid_flags` (it places a crate deliberately sunk into the floor) and `the_showcase_level_renders_every_core_prop_with_real_geometry`. |
| `prop_stress` | Prop Stress Test (dev) | `the_stress_level_batches_repeats_into_one_draw_per_model_and_cell` — ~150 repeated placements across nine models. |
| `vertical_diagnostic` | see above | — |

`prop_showcase`, `prop_stress`, `levels/asset_demo` and
`levels/asset_maintained` are **generated**: running
`python3 tools/levels/build_demo_levels.py` overwrites them. Change the
generator, not the JSON. Every other level here is hand-authored and safe to
edit directly.

## Why nothing else was removed

The Goal 6 audit traced every level to its references. No shipped level turned
out to be obsolete, redundant or unreferenced: each one is either the official
demo, a showcase, a residential level named by the Python suite, or a fixture
that an automated test loads by id. The only entries with no automated loader —
`texture_diagnostic` and `levels/asset_maintained` — are the sole coverage of
their cases (non-power-of-two and alpha textures; the maintained half of the
material-set comparison) and are referenced by the asset docs and the capture
tooling. Reclassifying or merging them would mean porting tests or dropping
coverage, so they were kept and indexed here instead, which is what made the
directory easier to read.
