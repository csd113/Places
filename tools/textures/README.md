# Environment texture toolkit

Development-only tooling that authors the shipped **environment surface PNGs**
(walls, floors, ceilings), the **decal sheets**, the **fixture faces** and the
diagnostic set, all registered as file-backed `asset_type: "texture"` /
`"decal"` / `"light"` entries in
[`../../assets/catalog.json`](../../assets/catalog.json).

The PNGs are the **authoritative runtime assets**: the game loads them at level
load, and editing or replacing one needs no Rust change and no recompilation.
This tool exists so the built-in artwork can be regenerated deterministically
and so every shipped sheet is checked against the texture budget.

The painter is pure stdlib — no PIL, no numpy — and byte-deterministic: running
it twice produces identical files, so the PNGs can be regenerated and diffed
like source.

## Commands

```sh
cd /path/to/Places
python3 tools/textures/build.py                              # (re)write every manifest sheet
python3 tools/textures/build.py --only core:tex_pool_tile_deck_01
python3 tools/textures/build.py --check                      # validate the shipped PNGs, no writes
python3 tools/textures/build.py --force --only core:tex_pool_tile_deck_01
```

A plain regeneration never downgrades shipped artwork: the shipped Office/Pool
surfaces and the NO DIVING sign ship at 1024x1024 while `office_art.py`,
`pool_art.py` and `decal_art.py` still paint 128x128 placeholder sheets, so
`build.py` **skips any sheet whose on-disk dimensions differ from its painter's
output** and requires `--force` to replace it. The remaining sheets (the
surface-response set, the fixture faces, the arrow/stripes decals and the
diagnostics) match their painters' dimensions and regenerate normally; the
painters themselves are deterministic, so a regeneration is diffable like
source.

A normal map (`extra_art.py`) is an ordinary surface sheet in the same asset
tree: RGB carries the tangent-space normal (`0..255` maps to `-1..1`), alpha is
unused, and the material that wants it names it with `normal_texture`. A
*cut-out* sheet like the grille is RGBA where the alpha channel is the shape.

`--check` reads `assets/catalog.json` and, for every file-backed sheet (surface
textures, decal sheets and fixture faces):

* the file exists below `assets/`;
* the bytes are a real PNG (signature, IHDR, IEND);
* the dimensions are non-zero and within the hard 1024x1024 limit;
* warns above the preferred 256x256 (a soft budget only: the shipped
  Office/Pool surfaces and the NO DIVING sign are intentionally 1024x1024) and
  for non-power-of-two dimensions (decal sheets and fixture faces must be POT);
* prints the parsed dimensions, e.g.
  `OK core:tex_carpet_beige_01: environment/office/textures/floors/carpet_beige_01.png 1024x1024`.

It exits non-zero on any error, never regenerates, and also warns when the
manifest in the art modules and the catalog's file-backed sheets drift apart.

## Tiling seams

`--check` above validates that a sheet *exists* and is within budget; it does
not look at its pixels. A repeated surface also has to join its opposite edges,
or every wall, floor and ceiling shows a grid of seams. `seam_repair.py` owns
that contract, with the same metric the Rust render test
(`src/render/tests.rs::test_shipped_surface_textures_tile`) enforces
independently:

```sh
python3 tools/textures/seam_repair.py --report assets/environment/office/textures/walls/wallpaper_stained_01.png
python3 tools/textures/seam_repair.py --check  assets/environment/office/textures/walls/wallpaper_stained_01.png
python3 tools/textures/seam_repair.py --repair assets/environment/office/textures/walls/wallpaper_stained_01.png
```

It compares the wrapped edge step with the sheet's own interior adjacent-pixel
step (raw and 3-tap-smoothed) on both axes and every channel, and accepts only
when the mean and p95 of the wrapped step stay inside a multiple of the
interior distribution plus a small absolute floor. A repair splits the sheet
into a low-frequency base and a high-frequency residual, cross-fades both into
a copy rolled by a per-sheet offset, and writes the result back with the same
dimensions, colour type and ancillary chunks. The parameters for each repaired
shipped sheet are pinned in the tool, so `--repair` reproduces the shipped file
byte for byte from the pre-repair artwork. `python3 -m unittest tests.test_package`
runs `--check` over every environment surface as a repository gate.

## Layout

| file | contents |
| --- | --- |
| `artkit.py` | the shared painting kit: `Canvas`, wrapped value noise (`hash01`, `tile_noise`, `tile_noise2`, `fbm`), `clamp`, `smoothstep`, colour mixes and the PNG writer |
| `office_art.py` | the Office wallpaper, carpet and panel ceiling, with their damaged variants |
| `pool_art.py` | the Pool deck, basin and wall tile and the sterile Pool ceiling |
| `lights_art.py` | the visible face of every built-in light fixture: the office fluorescent diffuser, the round pool downlight and the pool wall luminaire's lens |
| `extra_art.py` | the surface-response sheets: clear/dirty/tinted glass, a cut-out transfer grille, polished linoleum, a brushed-metal panel, a moulded-plastic panel, two tangent-space normal maps and the shared untextured white sheet |
| `decal_art.py` | the final Pool **NO DIVING** sign sheet (RGBA, transparent background) |
| `diagnostic_art.py` | the orientation/alpha/NPOT test sheets, never used by shipping levels |
| `seam_repair.py` | measures and repairs wrapped-edge seams in a shipped surface sheet (see below) |
| `build.py` | the CLI, the manifest merge and the `--check` gate |

Each art module exposes `ART = { "<logical id>": {"model": <path>, "build": <fn>} }`;
`build.py` merges them and refuses a duplicate id. A fixture's entry uses the
catalog's `light` id and its `kind` is `"light"` rather than `"texture"`, but it
is validated and regenerated exactly like a surface sheet.

## Shipped sheets

| id (`assets/catalog.json`) | file | dimensions | notes |
| --- | --- | --- | --- |
| `core:tex_wallpaper_yellow_01` | `environment/office/textures/walls/wallpaper_yellow_01.png` | 1024x1024 | printed office wallpaper, 2 m repeat |
| `core:tex_wallpaper_stained_01` | `environment/office/textures/walls/wallpaper_stained_01.png` | 1024x1024 | the same paper with water damage |
| `core:tex_carpet_beige_01` | `environment/office/textures/floors/carpet_beige_01.png` | 1024x1024 | institutional short-pile carpet, 2 m repeat |
| `core:tex_carpet_damp_01` | `environment/office/textures/floors/carpet_damp_01.png` | 1024x1024 | damp carpet |
| `core:tex_ceiling_panel_01` | `environment/office/textures/ceilings/ceiling_panel_01.png` | 1024x1024 | 2x2 suspended acoustic panels |
| `core:tex_ceiling_stained_01` | `environment/office/textures/ceilings/ceiling_stained_01.png` | 1024x1024 | one panel carries a tide stain |
| `core:tex_pool_tile_deck_01` | `environment/pool/textures/floors/pool_tile_deck_01.png` | 1024x1024 | 15 cm deck tile, 1.5 m repeat |
| `core:tex_pool_tile_basin_01` | `environment/pool/textures/floors/pool_tile_basin_01.png` | 1024x1024 | 10 cm basin tile, 1 m repeat |
| `core:tex_pool_tile_wall_01` | `environment/pool/textures/walls/pool_tile_wall_01.png` | 1024x1024 | 10 cm wall tile, 1 m repeat |
| `core:tex_pool_ceiling_01` | `environment/pool/textures/ceilings/pool_ceiling_01.png` | 1024x1024 | sterile painted panels, 2 m repeat |
| `core:fluorescent_panel_01` | `environment/office/textures/lights/fluorescent_panel_01.png` | 1024x512 | the office panel's twin-tube diffuser face (a fixture, not a tiling surface) |
| `core:pool_light_round` | `environment/pool/textures/lights/pool_light_round_01.png` | 128x128 | the round downlight's diffuser, seen face-on |
| `core:pool_light_wall` | `environment/pool/textures/lights/pool_light_wall_01.png` | 128x64 | the wall luminaire's ribbed lens face |
| `core:decal_no_diving_01` | `environment/pool/decals/no_diving_01.png` | 1024x1024 | RGBA cut-out safety sign (a decal, not a surface) |
| `core:decal_arrow_01` | `core/decals/arrow_01.png` | 128x128 | RGBA cut-out floor-direction arrow decal |
| `core:decal_stripes_01` | `core/decals/stripes_01.png` | 128x128 | RGBA cut-out diagonal hazard-band decal |
| `core:tex_glass_clear_01` | `core/textures/glass/glass_clear_01.png` | 1024x1024 | clear glazing: faint tint plus alpha, drawn with `alpha_mode: "blend"` |
| `core:tex_glass_dirty_01` | `core/textures/glass/glass_dirty_01.png` | 1024x1024 | unwashed glazing: blotchy alpha and a grime film |
| `core:tex_glass_tinted_01` | `core/textures/glass/glass_tinted_01.png` | 1024x1024 | tinted glazing at roughly half opacity |
| `core:tex_linoleum_01` | `core/textures/floors/linoleum_01.png` | 1024x1024 | polished speckled linoleum with faint seams |
| `core:tex_metal_panel_01` | `core/textures/walls/metal_panel_01.png` | 1024x1024 | brushed metal wall panel, two rivet rows |
| `core:tex_plastic_panel_01` | `core/textures/walls/plastic_panel_01.png` | 1024x1024 | moulded pale plastic wall panel |
| `core:tex_grille_01` | `core/textures/walls/grille_01.png` | 1024x1024 | RGBA cut-out transfer grille (metal slats over transparent openings) |
| `core:tex_normal_panel_01` | `core/textures/normals/normal_panel_01.png` | 1024x1024 | tangent-space normal map: soft moulded dimples |
| `core:tex_normal_brushed_01` | `core/textures/normals/normal_brushed_01.png` | 1024x1024 | tangent-space normal map: fine horizontal brushing |
| `core:tex_white_01` | `core/textures/white_01.png` | 2x2 | the shared untextured fallback sheet (solid opaque white, loaded at startup, never generated in Rust) |
| `core:tex_diagnostic_wall_01` | `diagnostic/textures/diagnostic_wall_01.png` | 128x128 | orientation-revealing, never shipped in a level |
| `core:tex_diagnostic_floor_01` | `diagnostic/textures/diagnostic_floor_01.png` | 128x128 | orientation-revealing |
| `core:tex_diagnostic_ceiling_01` | `diagnostic/textures/diagnostic_ceiling_01.png` | 128x128 | orientation-revealing |
| `core:tex_diagnostic_alt_01` | `diagnostic/textures/diagnostic_alt_01.png` | 96x64 | deliberately non-power-of-two |
| `core:tex_diagnostic_alpha_01` | `diagnostic/textures/diagnostic_alpha_01.png` | 128x128 | deliberately partly transparent |

The diagnostic sheets exist to prove arbitrary PNG dimensions (`alt`) and alpha
decode (`alpha`) on the real renderer — never for shipping levels.

## PNG budget guidance

The policy is declared in `src/assets.rs` (`ShippedTextureKind`) and mirrored by
`--check`; the numbers here and there must stay in step.

| rule                | value                                                       |
| ------------------- | ----------------------------------------------------------- |
| preferred           | ≤ 256x256, soft: the shipped Office/Pool surfaces and the NO DIVING sign are deliberately 1024x1024 |
| hard ceiling        | 1024x1024, enforced by `--check` and by the runtime decoder |
| decoded budget      | ≤ 4 MiB per surface sheet (one 1024x1024 RGBA8 sheet; `MAX_SURFACE_TEXTURE_BYTES`) |
| surface sheets      | square (the renderer samples them as square `tile_metres` cells); POT preferred, not required |
| fitted sheets       | decal sheets and fixture faces must be power-of-two (mipmapped, fitted UVs, ES 2.0) |
| colour space        | 8-bit RGBA (sRGB-ish); no gamma chunk is written or handled |
| alpha               | surfaces are opaque (alpha 255); decal sheets use alpha 0 for the cut-out |
| non-power-of-two    | surfaces load on the Mac (the 96x64 diagnostic proves it); avoid on ES 2.0 |

A 128x128 RGBA sheet is 64 KiB of pixels and a 1024x1024 sheet is the 4 MiB
budget; the shipped PNGs range from a few KiB to a couple of MiB compressed.
The renderer only decodes PNG, so there is no separate compression step and no
gamma/ICC handling: author in the working space and keep values near-neutral,
because materials multiply the texture by their `tint` and then by the baked
lighting.

## Adding a sheet

1. Add the painter and its `ART` entry in the theme's art module
   (`office_art.py`, `pool_art.py`, `lights_art.py`, `decal_art.py`, or a new
   module registered in `build.py`).
2. Add the matching catalog entry: `asset_type: "texture"`, `source: "file"`,
   `model`, `surface` — or `asset_type: "decal"`, `source: "file"`, `model` for
   a sign sheet, or a light fixture entry with `source: "file"` and `model`
   naming its visible face.
3. Point a `material` entry's `texture` field at a texture
   (`source: "definition"`, optional `tile_metres`, `tint`), place the decal id
   in a level's `decals`, or place the light fixture id in a level's
   `ceiling_lights`.
4. `python3 tools/textures/build.py`, then `python3 tools/assets/validate.py`
   and `python3 tests/test_package.py`.

Keep new art inside the budget table above; `--check` is the gate. Decal sheets
are drawn as cut-outs: the decal pass discards texels below alpha 0.5, so the
background is alpha 0 and the artwork is the silhouette plus its plate. Fixture
faces are fitted, not tiled: author the whole face, keep the PNG opaque, and
match the sheet's aspect to the face the fixture maps (see the mapping table in
`src/render/common/fixtures.rs`).
