# Office environment

The initial office collection: the classic liminal materials, the fluorescent
ceiling fixture and clearly office-oriented furniture.

Every surface material is a **data definition** that names an external PNG
texture. The material owns the tiling/tint; the texture owns the
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
| `core:fluorescent_panel_01` | light | `core:fluorescent_panel_01` (1024x512 face) | `textures/lights/fluorescent_panel_01.png` |
| `core:desk`, `core:chair`, `core:cabinet`, `core:water_cooler`, `core:vending_machine` | prop | embedded in each GLB | `props/models/*.glb` |

## Surface artwork

The six PNGs are the authoritative shipped Office artwork. All six export
functions load these PNGs, including forced texture builds.
They ship as 1024x1024 8-bit RGB sheets, opaque, and tileable in both
directions (the wrapped edges are gated by `tools/textures/seam_repair.py
--check`; see [Texture seam repair](#texture-seam-repair) below). They stay
pale and near-neutral, because the material `tint` and the baked lighting
multiply into the sampled texel; the carpet is the deliberate exception (its
material has no tint), so it is painted at the warm-brown albedo.

* `wallpaper_yellow_01` — remade pale cream stock: fine vertical pinstripes,
  staggered faded lozenges and fibrous paper tooth per 2 m cell. Repetitive
  and commercial rather than ornate; replaces the former double chevrons.
* `wallpaper_stained_01` — the same paper with restrained water damage: soft
  damp fields and a few vertical runs. No dark outlines, so the damage does not
  turn into a repeating pattern.
* `carpet_beige_01` — short-pile carpet: low-frequency mottle, fine directional
  fibre and 2 px pile loops. **There is no metre checker anywhere**: the sheet
  is painted so the demo's office carpet keeps its brightness and warmth.
* `carpet_damp_01` — the same dense short pile, with organically spreading, flattened
  dark patches and soft tide edges rather than a uniform darkened copy.
* `ceiling_panel_01` — a 2x2 grid of 1 m suspended acoustic panels (2 px T-bar
  plus a 1 px shadow groove), slightly yellowed, with per-panel tone variation
  and pinhole speckle.
* `ceiling_stained_01` — the same grid with irregular water tide marks on three
  panels. Damage stays inside the panels; the acoustic pores and T-bars remain
  readable. The grid and sheet orientation are unchanged.

`tools/textures/office_art.py` now loads all six committed sheets rather than
substituting a low-resolution painter. `tools/textures/lights_art.py` loads the
Office fixture's fitted 2:1 PNG. Its twin ivory tubes, socket ends and fine
prismatic diffuser ribs read within the existing recessed mesh housing.
`tools/textures/build.py --check` validates without changing artwork.

## Furniture and appliance sources

The concept-art pass refines the existing five props in place; no ids or
catalog dimensions changed. Each retains its opaque UV atlas layout, a
1024-square authored `*_master.png` and a derived native 256-square `*.png`
beside its GLB. Runtime only loads the embedded native PNG.

| Prop | Construction | Triangles |
| --- | --- | ---: |
| `core:desk` | warm laminate desktop with a narrow bevel, two painted metal drawer pedestals, modesty panel, pencil drawer and bridge pulls | 356 |
| `core:chair` | retained shaped seat, lumbar pad, caster base and repaired geometry; new woven charcoal upholstery | 704 |
| `core:cabinet` | two broad filing drawers with reveals, label holders, bridge handles and a recessed plinth within the original low cabinet footprint | 212 |
| `core:water_cooler` | off-white cabinet with an open tap recess, two controls, drip tray and a continuous eight-sided ribbed blue bottle | 556 |
| `core:vending_machine` | inset mountain/DRINKS fascia, separate dark frame, raised controls, coin bezel and delivery lip | 252 |

```sh
python3 tools/props/build_office_textures.py
python3 tools/props/build.py --only core:desk core:chair core:cabinet core:water_cooler core:vending_machine
python3 tools/props/build_office_textures.py --check
python3 -m unittest tests.test_office_assets
```

`tools/props/parts/office_refined.py` owns this authoring. The chair uses its
shipped GLB as the existing authored geometry source, preserving the later
repairs rather than rebuilding a weaker primitive version. Its export retains
positions, indices, UVs and vertex colours while embedding the new artwork.
The other four props are reproducible from closed primitives and source PNGs.

The prompts and detailed audit/validation results are in
[`office-refinement-prompts.json`](../../../docs/reports/office-refinement-prompts.json)
and [`office-refinement.md`](../../../docs/reports/office-refinement.md).

The official demo `../../levels/places_demo.json` exercises the set: a warm
office reception and workroom on the yellow wallpaper and panel ceiling, the
stained/damp variants in the areas the building has given up on, sparse desks
and task chairs, cabinets, a water cooler and floor decals.

Generic props (couch, bed, plants, utilities, ...) are deliberately **not**
listed here: they belong to no theme and live under `../../core/`.

## Texture seam repair

The 1024x1024 sheets are the authoritative artwork; the historical 128x128
helpers in `tools/textures/office_art.py` are not exported. All six sheets wrap
cleanly on both axes. Use the existing cross-fade tool when repairing edges:

```sh
python3 tools/textures/seam_repair.py --repair <path-to-sheet.png>
```

The tool keeps the colour type, exact dimensions and ancillary chunks. The new
stained wallpaper uses `--band 32 --residual-band 12 --radius 12 --offset 16`;
clean wallpaper passes without repair. Damp carpet uses band 80, residual 12,
radius 32 with its existing tuned offsets; stained ceiling uses band 16,
residual 6, radius 6, offset 16. `--report` prints the wrapped edge
step against the sheet's own interior adjacent-pixel step; `--check` gates
every sheet on

    mean(wrap) <= 1.60 * mean(interior) + 1.0
    p95(wrap)  <= 2.20 * p95(interior)  + 3.0

for both the raw and the three-tap-smoothed profiles, and exits non-zero on a
failure.

All six surface exporters preserve the committed 1024-square artwork. A plain
or forced texture build cannot substitute the old 128-square helpers. The
fixture exporter likewise preserves its committed 1024x512 aspect and artwork.

The refined damp carpet uses the existing seam tool's 80/12-pixel bands,
radius 32, offsets 184/831. The refined stained ceiling uses bands 16/6,
radius 6 and offset 16 on both axes to preserve its edge/grid module. These
are one-time source repairs, not runtime processing.

## Native visual review

`tests/fixtures/levels/office_asset_review.json` is a small connected maintained
and damaged Office review space using all five props and the existing lighting.
It has no custom renderer behavior, extra asset ids or gameplay changes.
Compile into a scratch asset root and run `tools/bench/capture_office.py`; see
its `--help` and the report. Captures cover furniture, appliances, ceilings,
transitions and damaged surfaces at pinned High/Low settings. Generated images,
logs, packages and comparison data belong in `target/office-refinement/`.
