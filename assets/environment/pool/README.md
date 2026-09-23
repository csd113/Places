# Pool environment

The Pool content pack: a clean, relatively new, sterile institutional pool.
Everything is pale and cool — off-white tile, a neutral painted ceiling, white
resin furniture, chrome and dull-silver metalwork and pale privacy curtains —
with only light wear, because the Pool is empty and quiet, not abandoned.

The pack is an ordinary set of catalog entries; nothing about the Office/Pool
split restricts where an asset may be placed (see
[`../../README.md`](../../README.md)).

## Surface textures

| texture id | file | dimensions | tile_metres | what it is |
| --- | --- | --- | --- | --- |
| `core:tex_pool_tile_deck_01` | `textures/floors/pool_tile_deck_01.png` | 1024x1024 | 1.5 | 10x10 grid of 15 cm commercial deck tiles (102.4 px per tile) |
| `core:tex_pool_tile_basin_01` | `textures/floors/pool_tile_basin_01.png` | 1024x1024 | 1.0 | 10x10 grid of 10 cm basin tiles, a shade cooler |
| `core:tex_pool_tile_wall_01` | `textures/walls/pool_tile_wall_01.png` | 1024x1024 | 1.0 | 10x10 grid of 10 cm wall tiles, the palest of the family |
| `core:tex_pool_ceiling_01` | `textures/ceilings/pool_ceiling_01.png` | 1024x1024 | 2.0 | 2x2 painted panels (512 px = 1 m), fine joints, screw dimples |

All four are opaque 8-bit RGBA, tileable in both directions, and deterministic:
`tools/textures/build.py --only <id>` regenerates them from
[`pool_art.py`](../../../tools/textures/pool_art.py). The grout/panel joints are
one pixel, close in value to the field (12.5 % darker for the tile joints,
10 % for the ceiling seams) with a 1 px bevel, so they read as joints and never
as a debug grid. Tone comes from per-tile jitter (one tile in seven slightly
darker), a gentle low-frequency field and a barely-visible speckle.

The 1024x1024 sheets are the authoritative artwork, not the 128 px painter
output: `tools/textures/build.py` skips a sheet whose shipped dimensions
differ from its painter's output, so a plain run leaves them untouched and only
`--force` would replace the shipped sheets with the 128 px output. The wall
tile's left-to-right wrap step measured about three times its own interior
variation (a visible vertical seam where the tile grid re-met itself); it was
repaired in place with
`python3 tools/textures/seam_repair.py --repair assets/environment/pool/textures/walls/pool_tile_wall_01.png`
and the wrapped edge is now gated by both the Rust surface-tiling test and
`python3 tools/textures/seam_repair.py --check`. The tool pins this sheet's
repair parameters, so `--repair` reproduces the shipped file deterministically
from the pre-repair artwork.

The fixture faces are external artwork too: `core:pool_light_round` ships
`textures/lights/pool_light_round_01.png` (128x128) as its recessed downlight
diffuser and `core:pool_light_wall` ships
`textures/lights/pool_light_wall_01.png` (128x64) as its luminaire lens. The
mesh around them is generated; author a wall fixture with `"mount": "wall"` and
a world `"y"`, exactly as before.

## Materials

| material id | texture | tile_metres |
| --- | --- | --- |
| `core:pool_tile_deck_01` | deck tile | 1.5 |
| `core:pool_tile_basin_01` | basin tile | 1.0 |
| `core:pool_tile_wall_01` | wall tile | 1.0 |
| `core:pool_ceiling_01` | ceiling panel | 2.0 |

## Props

All nine are modelled in [`tools/props/parts/pool.py`](../../../tools/props/parts/pool.py):
1 unit = 1 m, origin on the floor-contact centre, `+Z` front, one embedded
128x128 texture each, well inside the triangle budget. Moulded resin parts are
built from four-sided tapered blocks, metal work from eight-sided stock, and
cloth from a double-sided folded ribbon, so the three material families never
read alike.

| prop id | size [w, h, d] | notes |
| --- | --- | --- |
| `core:pool_table` | 0.80 x 0.74 x 0.80 | white resin tray top with a 4 cm rim over a 12 mm tray floor, a moulded apron, four tapered square legs and a low perimeter stretcher ring |
| `core:pool_chair` | 0.52 x 0.85 x 0.55 | the table's sibling: 45 cm seat with a rolled front edge, apron, tapered front legs, rear legs raked 8 degrees and a 13 degree slatted back |
| `core:pool_ladder` | 0.55 x 2.20 x 0.45 | chrome rails curving 0.33 m out over the deck edge on a 0.10 m radius, four non-skid treads on a 0.305 m pitch and vinyl foot boots |
| `core:pool_curtain_straight` | 1.20 x 2.60 x 0.22 | two 48 mm posts on square foot plates, an extruded top track and a ten-pleat gathered panel |
| `core:pool_curtain_end` | 0.60 x 2.60 x 0.22 | one post and a five-pleat panel closing a run |
| `core:pool_curtain_corner` | 0.60 x 2.60 x 0.60 | shared corner post, one tighter-packed panel per leg |
| `core:pool_guardrail_straight` | 2.00 x 1.05 x 0.08 | **one** waist-high Ø42 rail at 0.98 m on three Ø48 posts with turned caps and bolted flanges |
| `core:pool_guardrail_end` | 0.60 x 1.05 x 0.08 | short single-rail return terminating a run |
| `core:pool_guardrail_corner` | 0.60 x 1.05 x 0.60 | L section, one rail per leg turning through the shared corner post |

The guardrail is deliberately a *guard rail* and not a fence: a single rail on
posts, no second rail line and no infill. Its 8 cm catalogue depth is the
flange, which is sized across the rail it carries.

Curtain and guardrail modules compose on a 0.6 m bay grid: a straight section's
posts sit inboard by exactly their own radius (curtains: by the foot plate's
half-width), so a module's post surface, and a guardrail's flange, are flush
with the module's catalogue edge and two modules placed edge to edge meet
piece to piece. A section rotated 90 degrees needs its level `"size"` written
with x/z swapped.

## The Pool content

The Pool family is the one the official demo ends in: `places_demo` walks from
the office into the red stair hall and out onto the pool deck, past the
guardrail runs and the curtain cubicle line. A dedicated regression fixture in
`tests/fixtures/levels/pool_showcase.json` composes the family on its own: a
16 x 11 m pool room with a 9 x 4.5 m empty basin (deck at 0, a -0.35 m walk-in
step, the basin floor at -1.5 m), a corridor and a changing bay, cool round
ceiling lights plus four wall luminaires, the patio table and two chairs, and
the final NO DIVING decal on the deck and on the north wall. Stage it in a
`levels/` directory to boot it with `LIMINAL_LEVEL=pool_showcase`.

## Decal artwork

`core:decal_no_diving_01` is an external PNG cut-out
(`decals/no_diving_01.png`, 1024x1024 RGBA, generated by
[`decal_art.py`](../../../tools/textures/decal_art.py)): a white plate with a
red rim, the prohibition pictogram and bold lettering, with alpha 0 around the
plate so the decal pass can discard it. Replace the PNG and restart to change
the sign; no Rust change and no recompilation. The sheet is uploaded as its own
decal texture, and the renderer corrects the in-plane orientation so the
artwork reads exactly as it does in an image viewer (verified for a floor and a
wall placement; `render::decal_uv_rect_full` is pinned by a unit test).

## Known renderer behaviour observed while verifying this pack

Two defects found while placing this content were fixed during integration and
are now covered by tests: wall-mounted decals were mirrored (`render::tests::external_decal_sheets_pin_their_world_orientation`)
and the generated decal atlas drew its patterns in transposed cells, so
`core:decal_arrow_01` sampled an empty cell and `core:decal_stripes_01` sampled
the arrow (`render::tests::generated_decal_atlas_cells_match_their_sheet_slots`).
