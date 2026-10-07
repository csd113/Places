# Pool environment

The concept source is [Places: Quiet Indoor Pool Asset Sheet](Places_%20Quiet%20Indoor%20Pool%20Asset%20Sheet.png).
The [2026-10-07 visual pass](../../../docs/style-upgrade-20261007/pool/README.md)
records the object inventory, matched native captures, validation and costs.

Pool remains a quiet commercial interior: pale speckled deck ceramic, blue
basin ceramic, pale large-format wall tile, fine-seamed painted ceiling, white
moulded resin, silver fittings and warm off-white privacy cloth. Wear is light.

| Surface | ID | Source | Repeat / construction |
| --- | --- | --- | --- |
| Deck | `core:pool_tile_deck_01` | 1024² | 10×10 tiles / 1.5 m, 15 cm tile with restrained mineral flecks |
| Basin | `core:pool_tile_basin_01` | 1024² | 10×10 tiles / 1 m, distinctly blue 10 cm ceramic |
| Wall | `core:pool_tile_wall_01` | 1024² | 10×10 tiles / 2 m, pale 20 cm ceramic |
| Ceiling | `core:pool_ceiling_01` | 1024² | 2×2 panels / 2 m, fine seams without office staining |
| Coping | `pool:coping_01` | 1024² | 5×5 pale tiles / 1.5 m, non-colliding 45 mm cap strips |
| Tile band | `pool:band_01` | 1024² | 10×10 blue tiles / 1 m, thin wainscot and construction transitions |
| Metal trim | `pool:metal_01` | 1024² | opaque cool powder coat, restrained existing brushed normal at 0.18 strength |
| Water | `core:water_pool_01` | existing 1024² | existing working water texture, material and volume mechanics retained |

All surface sheets are square, opaque and seam-gated. The round fixture face
remains 1:1 at 1024²; the wall lens remains its required 2:1 at 1024×512.
`pool:decal_lane_01` is a fitted 1024×32 RGBA basin marking, placed at 32:1.
The existing 1024² NO DIVING cutout and water artwork are retained.

| Prop family | Catalog bounds (m) | Construction |
| --- | --- | --- |
| `core:pool_table` | .80 × .74 × .80 | round rolled resin top, recessed skirt, four square tapered legs |
| `core:pool_chair` | .52 × .85 × .55 | moulded arms, crowned vertically slotted back, rolled seat and four floor-contact feet |
| `core:pool_ladder` | .55 × 2.20 × .45 | closed inverted-U rails returning to bolted deck flanges; four dark tread inserts |
| `core:pool_guardrail_*` | existing module bounds | one waist-high rail at .98 m; flanged posts; straight, end and corner modules |
| `core:pool_curtain_*` | existing module bounds | 2 mm closed pleated cloth shell, gathered header, hanging tabs, stitched weighted hem |
| `pool:pool_bench` | 1.60 × .45 × .38 | four grey resin slats on two metal trestles, floor-contact foot pads |
| `pool:pool_drain` | .60 × .014 × .16 | dark closed recess, metal perimeter and fifteen raised grate bars |
| `pool:pool_service_door` | 1.00 × 2.10 × .15 | blue static leaf, deep frame, threshold, pull handle and recessed six-slat vent |
| `core:rubber_duck`, `core:hot_tub` | existing bounds | retained identity/mechanics; neither was remade by this pass |

Model UVs use fitted 2×2 cells. Each remade/added model keeps a 1024²
`*_master.png` beside its 256² native PNG. The offline authoring script
`tools/textures/author_pool.py` creates those masters and downsamples once with
Lanczos; ordinary texture builds load the committed PNGs without repainting.
The game loads GLBs and PNGs only. It never runs the painter.

Rebuild selected props with `python3 tools/props/build.py --only <id>`.
All current Pool builders are in `tools/props/parts/pool_remade.py`, registered
by `pool.py`; the retained hot tub and duck use their existing builders.
Native model textures stay at 256² (128² on Low); no 1024² image is embedded
in the GLBs. The largest remade model is 612 triangles.

`tools/levels/refine_pool.py` idempotently updates only Pool content in the
connected demo. Water volumes, climb volumes, entity placements, other rooms
and the office-facing wall materials/glazing remain intact. Submerged treads
are real floor regions with 300 mm risers. Dry coping caps keep 2–4 mm grout
joints and stay outside the recessed walking surfaces. The reference benches
sit by the changing screens and at the dry perimeter. The two former blank
notice boards are now static blue service leaves. The existing potted plant
is reused beside the resin furniture.
