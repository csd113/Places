# Environment texture toolkit

Development-only tooling that authors the shipped **environment surface PNGs**
(walls, floors, ceilings and the diagnostic set) registered as
`asset_type: "texture"` entries in [`../../assets/catalog.json`](../../assets/catalog.json).

These are **seed assets, not final art**: they are deterministic, tileable
approximations of the renderer's original procedural materials so the runtime
can move to file-backed textures now. Goal 5 replaces them with real artwork
and this tool can then shrink to its `--check` half.

The painter is pure stdlib — no PIL, no numpy — and byte-deterministic: running
it twice produces identical files, so the PNGs can be regenerated and diffed
like source. See [`build.py`](build.py) for the painters; the noise helpers are
an approximate port of the texture functions in `src/render.rs`.

## Commands

```sh
cd /path/to/Places
python3 tools/textures/build.py           # (re)write every manifest texture
python3 tools/textures/build.py --check   # validate the shipped PNGs, no writes
```

`--check` reads `assets/catalog.json` and, for every texture asset:

* the file exists below `assets/`;
* the bytes are a real PNG (signature, IHDR, IEND);
* the dimensions are non-zero and within the hard 1024x1024 limit;
* warns above the preferred 256x256 and for non-power-of-two dimensions;
* prints the parsed dimensions, e.g.
  `OK core:tex_carpet_beige_01: environment/office/textures/floors/carpet_beige_01.png 128x128`.

It exits non-zero on any error, never regenerates, and also warns when the
manifest in `build.py` and the catalog's texture entries drift apart.

## PNG budget guidance

| rule                | value                                                       |
| ------------------- | ----------------------------------------------------------- |
| preferred           | ≤ 256x256 (the shipped seed set is 128x128)                 |
| hard ceiling        | 1024x1024                                                   |
| colour space        | 8-bit RGBA (sRGB-ish); no gamma chunk is written or handled |
| alpha               | allowed; the renderer decodes RGBA, opaque art uses alpha 255 |
| power of two        | preferred for OpenGL ES 2.0 portability                     |
| non-power-of-two    | loads on the Mac (e.g. 96x64 diagnostic); avoid on ES 2.0   |

A 128x128 RGBA sheet is 64 KiB of pixels; the compressed PNGs here are a few
KiB each. The renderer only decodes PNG, so there is no separate compression
step and no gamma/ICC handling: author in the working space and keep values
near-neutral, because materials multiply the texture by their `tint`.

## Layout

| id (`assets/catalog.json`)        | file                                                          | dimensions | class / theme        |
| --------------------------------- | ------------------------------------------------------------- | ---------- | -------------------- |
| `core:tex_wallpaper_yellow_01`    | `environment/office/textures/walls/wallpaper_yellow_01.png`   | 128x128    | environment / office |
| `core:tex_wallpaper_stained_01`   | `environment/office/textures/walls/wallpaper_stained_01.png`  | 128x128    | environment / office |
| `core:tex_carpet_beige_01`        | `environment/office/textures/floors/carpet_beige_01.png`      | 128x128    | environment / office |
| `core:tex_carpet_damp_01`         | `environment/office/textures/floors/carpet_damp_01.png`       | 128x128    | environment / office |
| `core:tex_ceiling_panel_01`       | `environment/office/textures/ceilings/ceiling_panel_01.png`   | 128x128    | environment / office |
| `core:tex_ceiling_stained_01`     | `environment/office/textures/ceilings/ceiling_stained_01.png` | 128x128    | environment / office |
| `core:tex_diagnostic_wall_01`     | `diagnostic/textures/diagnostic_wall_01.png`                  | 128x128    | diagnostic / —       |
| `core:tex_diagnostic_floor_01`    | `diagnostic/textures/diagnostic_floor_01.png`                 | 128x128    | diagnostic / —       |
| `core:tex_diagnostic_ceiling_01`  | `diagnostic/textures/diagnostic_ceiling_01.png`               | 128x128    | diagnostic / —       |
| `core:tex_diagnostic_alt_01`      | `diagnostic/textures/diagnostic_alt_01.png`                   | 96x64      | diagnostic / —       |
| `core:tex_diagnostic_alpha_01`    | `diagnostic/textures/diagnostic_alpha_01.png`                 | 128x128    | diagnostic / —       |

The office sheets carry the same look as the renderer's generated materials:
pale near-neutral wallpaper stripes (the wall tint multiplies them gold), a
short-pile carpet with the historical 1 m checker tint baked into its four
64x64 quadrants (top-left bright), and a 2x2 panel ceiling with a 3 cm T-bar
grid. The stained variants reuse the same base plus water damage.

The diagnostic sheets are deliberately artificial and orientation-revealing
(red/green/blue/yellow corner markers, arrows, rings) and exist to prove
arbitrary PNG dimensions (`alt`, 96x64) and alpha decode (`alpha`) on the real
renderer — never for shipping levels.

## Adding a texture

1. Add the painter and its `MANIFEST` entry in `build.py` (id -> catalog model
   path + builder).
2. Add the matching `asset_type: "texture"` catalog entry with `source: "file"`,
   `model`, and `surface`.
3. Point a `material` entry's `texture` field at it
   (`source: "definition"`, optional `tile_metres`, `tint`).
4. `python3 tools/textures/build.py`, then `python3 tools/assets/validate.py`
   and `PYTHONDONTWRITEBYTECODE=1 python3 tests/test_package.py`.

Keep new art inside the budget table above; `--check` is the gate.
