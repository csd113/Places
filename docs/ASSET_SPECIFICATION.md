# Places Asset Specification

Repository-wide checks: [authoritative desktop verification](VERIFICATION.md).

This document is the canonical specification for every visual asset Places
draws from an image: world surface textures, decals, fixture faces, prop and
entity textures, normal maps, emissive masks and the non-runtime imagery that
ships with the repository (the app icon, documentation screenshots).

It is written for two audiences:

* people authoring or replacing artwork;
* AI agents generating, replacing, resizing or converting artwork.

Read it before touching a production asset. Every rule below is derived from
the current implementation: the renderer and its shaders, the material
resolver, the UV and geometry generators, the model loader, the texture
loader, the asset catalog, the tests and the tooling. Where a rule is enforced
automatically, the enforcing command is named. Where it is policy or
convention only, that is stated too.

> **Core principle.** Texture resolution is not an asset contract. Aspect
> ratio, orientation, UV layout, transparency behaviour and the relationship
> between an image and its geometry *are* contracts, and they must not change
> unless the corresponding engine or geometry definition is intentionally
> updated.
>
> **Never infer an asset contract from the dimensions of the current PNG.
> Inspect how the asset is actually used.**

---

## 1. Purpose

Places loads its visual assets at level load and never regenerates production
artwork at runtime. The renderer samples images through a small number of
well-defined paths (tiling world surfaces, fitted fixture faces, fitted decal
sheets, fitted model textures, emissive masks), and each path carries its own
contract for aspect ratio, alpha, tiling, filtering and orientation.

Without a written specification, those contracts are discoverable only by
reading the renderer. This document records them once so that:

* an asset can be replaced without inspecting the renderer;
* a new asset class is introduced deliberately rather than accidentally;
* an AI agent never guesses "make it 1024×1024" for an image whose contract
  is 2:1, 1:1, model-defined or arbitrary;
* mechanical rules that are already enforced automatically are distinguished
  from rules that depend on an author's judgement.

If an asset's required behaviour is not covered here, the asset class does not
exist yet. Inspect the implementation, then update this document before
introducing it.

---

## 2. General rules

1. **Production visual assets are real files in the repository.** Every
   texture, decal, fixture face, normal map and material image is a committed
   PNG under `assets/`, referenced through `assets/catalog.json` by logical id.
   Prop and entity models also embed their runtime PNGs for self-contained
   loading (see §8). Every embedded image has an equivalent standalone PNG
   under `assets/`; the audit checks decoded pixels and dimensions, allowing
   lossless differences in PNG compression or metadata.

2. **No production texture imagery is generated from source code at runtime.**
   Do not paint textures in Rust, in shaders, in embedded pixel arrays or in
   draw commands, and do not synthesize a missing texture on demand when the
   application or a level loads. The shared untextured white sheet is a real
   committed PNG like every other texture (`core:tex_white_01`, §12.2). The
   HUD font atlas, internal decal atlas and diagnostic fallback sheets are
   committed PNGs with fixed layouts (§12.3). Lightmap atlases and reflection
   probes are computed lighting data, rather than authored texture imagery.

3. **PNG is the only raster format the runtime accepts.** The decoder
   signature-checks every texture and rejects anything that is not a PNG
   (`src/materials/image.rs::decode_png`). There is no JPEG, WebP, TGA, BMP,
   EXR or KTX path.

4. **Aspect ratio and source resolution are separate concepts.**
   *The aspect ratio* is the shape and UV layout the engine or the model
   expects (for example `2:1`, `1:1`, `model-defined`, `arbitrary`). *The
   source resolution* is how many pixels that shape is authored at (for
   example `1024×512`). An asset may be re-authored at a higher resolution as
   long as its aspect ratio, UV layout, orientation and alpha behaviour stay
   the same. See §14 and §20.

5. **UV layouts and orientation are part of the contract.** A fitted sheet
   (fixture face, decal, model texture) is sampled by UV coordinates that live
   in geometry or in the model file. Changing the artwork's orientation or
   internal layout changes what those coordinates sample. Do not rotate,
   mirror or re-pack a fitted sheet unless the geometry or model is updated
   with it.

6. **Never stretch artwork into a different ratio.** A `tile_metres` cell is
   square on both axes, a fixture face has its own fixed aspect, and a decal
   quad is as wide and tall as the level places it. Artwork authored in a
   different ratio will be stretched, and nothing at runtime warns about it.

7. **Alpha is only used where the material system supports it.** A surface's
   alpha channel is ignored unless its material authors `alpha_mode: "cutout"`
   or `"blend"`. Decal sheets are always alpha cut-outs. Fixture faces and
   prop/entity textures are opaque; their alpha channel is ignored. See §16.

8. **Source artwork should retain as much quality as the hard limits allow.**
   The hard ceiling is 1024 px on either edge, for every PNG, enforced by the
   runtime decoder. Store the best source within that limit; the engine
   decides how much of it reaches the GPU. Do not author a second, smaller
   asset set for a lower quality level — every level uses the same files
   (see §14).

9. **Tileable sheets must not introduce seams.** A tiling surface must join
   its own edges cleanly in both directions (§13). This is measured by
   `tools/textures/seam_repair.py --check` and by a Rust render test.

10. **Do not repack a model texture.** Model UVs are baked into the `.glb`.
    Uniformly resizing a model texture is safe because UVs are normalized
    fractions; moving or reordering texture regions invalidates the model's
    UVs and requires rebuilding the model (§8).

11. **The catalog is the registry.** Levels reference logical ids, never file
    paths. A new surface material, decal or fixture is a catalog entry plus a
    PNG; the runtime resolves the file from the catalog.

---

## 3. Asset directory structure

```
assets/
  catalog.json                       authoritative registry of logical assets
  README.md                          asset-system documentation
  levels/                            shipped level files (assets referenced by id)
  core/                              generic, theme-independent content
    decals/*.png                     shared decal sheets
    props/models/*.glb               shared props (embedded textures)
    textures/
      glass/*.png                    glass surface sheets
      floors/*.png                   generic floor sheets
      walls/*.png                    generic wall sheets
      effects/*.png                  effect surface sheets
      metal/*.png                    metal surface sheets
      normals/*.png                  tangent-space normal maps
      white_01.png                   shared untextured fallback sheet (§12.2)
  environment/
    <theme>/                         one directory per theme: office, pool, home
      props/models/*.glb             theme props (embedded textures)
      textures/
        walls/*.png                  wall surface sheets
        floors/*.png                 floor surface sheets
        ceilings/*.png               ceiling surface sheets
        lights/*.png                 light fixture faces
        doors/*.png                  door leaf surface sheets
        water/*.png                  water surface sheets
      decals/*.png                   theme decal sheets
  entities/
    <id>/model/<id>.glb              entities (embedded textures)
    <id>/textures/*.png              authoring source sheets kept beside the model
  diagnostic/
    textures/*.png                   engine test artwork; never used by shipped levels
```

Non-runtime imagery elsewhere:

```
docs/screenshots/*.png               documentation captures
icon.png                             application icon
```

Rules:

* **Theme directories organize, they never restrict.** Any material, prop,
  decal or fixture may be used in any level regardless of theme. Generic
  content lives under `assets/core/`; themed content under
  `assets/environment/<theme>/`.
* **Surface textures live in `textures/{walls,floors,ceilings}/`** according
  to the surface they were painted for. The directory is organisational; the
  catalog's `surface` field is documentation/validation only, and what the
  renderer reads is the material's `texture`, `tile_metres`, `tint` and
  alpha/response fields. A level may use any material on any surface. Door,
  water, effect and metal sheets follow the same convention in their own
  `textures/` subdirectories.
* **Fixture faces live in `textures/lights/`.** They are catalog `light`
  entries, not `texture` entries; the fixture PNG is the `model` of the light.
* **Decal sheets live in `decals/`** — `assets/environment/<theme>/decals/`
  for theme art, `assets/core/decals/` for shared markings. They are catalog
  `decal` entries; the PNG is the `model`.
* **Prop and entity textures are embedded in the GLB** under
  `props/models/` (props) or `entities/<id>/model/` (entities). Standalone
  native source PNGs live beside prop GLBs or in the entity's `textures/`
  directory. Family members may share a source. The five house families keep
  both `house_0N_native_128.png` and `house_0N_native_256.png` alongside their
  larger `house_0N_materials.png` masters. There is no `props/textures/` directory.
* **Normal maps live in a texture directory like any other sheet**
  (`assets/core/textures/normals/`), and are named by a material's
  `normal_texture` field.
* **Emissive masks would live in a texture directory like normal maps** and
  are named by a material's `emissive_mask` field. No shipped material
  currently authors one.
* **Naming**: surface/decal texture ids use a `tex_` prefix
  (`core:tex_wallpaper_yellow_01`) while the material drops it
  (`core:wallpaper_yellow_01`); numbered variants end `_01`; files use the
  same lower-case `_01` naming. This convention is not enforced by tooling.
* **A PNG is claimed by exactly one catalog entry.** Two assets cannot share
  one file. (Three pool curtain models and three guardrail models
  intentionally contain identical embedded texture *bytes* — see §8.3.)

---

## 4. Texture class table (quick reference)

The aspect ratio is the layout contract. "Preferred source resolution" is the
authored size used by current production assets and the natural size for new
art; "Hard maximum" is 1024 px per edge for every class unless noted.
"Minimum" records what, if anything, the repository enforces as a floor —
currently nothing enforces a minimum for any class.

| Asset class | Example | Aspect ratio | Preferred source resolution | Minimum | Alpha | Tileable | UV contract | Notes |
|---|---|---|---|---|---|---|---|---|
| Wall surface sheet | `wallpaper_yellow_01.png` | **1:1** | 1024×1024 (shipped); soft warning above 256 | none enforced | RGB shipped; ignored unless material is `cutout`/`blend` | **yes, both axes** | world metres ÷ `tile_metres` | square because the tile cell is square |
| Floor surface sheet | `carpet_beige_01.png` | **1:1** | 1024×1024 (shipped) | none enforced | RGB shipped (carpet); ignored by default material | **yes, both axes** | world `(x, z)` ÷ `tile_metres` | carpet has no tint; painted at warm albedo |
| Ceiling surface sheet | `ceiling_panel_01.png` | **1:1** | 1024×1024 (shipped) | none enforced | RGB shipped; ignored by default material | **yes, both axes** | world `(x, z)` ÷ `tile_metres` | ceiling material authors a grey tint |
| Core shared sheet (glass/floor/wall) | `glass_clear_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | per material: `blend` glass, `cutout` grille, opaque otherwise | **yes, both axes** | world metres ÷ `tile_metres` | seam-gated with the environment set |
| Steam effect sheet | `steam_01.png` | **1:1** | 256×256 (current production); 128×128 painter output is contract-valid | none enforced | **required**: a soft falloff reaching alpha 0 at every edge | sampled `REPEAT`, but its own edge alpha is 0 | one sheet per billboard; UV span = particle size ÷ the material's `tile_metres` | catalog `texture`; bound by the `core:steam_01`-style blended material; see §7.1 |
| Shared white sheet | `white_01.png` | **1:1** | 1024×1024 (current production); a flat fill, so any POT size is contract-valid | none enforced | opaque white | no | no authored UV contract: it is a flat fill bound wherever a surface is untextured | engine fallback loaded once at startup; see §12.2 |
| Normal map | `normal_panel_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | alpha unused | **yes, both axes** | same UV as the albedo it augments | Surface quality class |
| Fluorescent panel face | `fluorescent_panel_01.png` | **2:1** | 1024×512 (current production) | none enforced | ignored (face is opaque) | no | full sheet fit to the 1.12 × 0.56 m diffuser aperture; `u` across the width, `v` across the depth | POT both edges; replacement must stay 2:1 |
| Round downlight face | `pool_light_round_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius | POT both edges |
| Wall luminaire face | `pool_light_wall_01.png` | **2:1** | 1024×512 (current production); 128×64 painter output is contract-valid | none enforced | ignored | no | full sheet; `u` across 0.4 m width, `v` up 0.2 m height | POT both edges |
| Flush-mount diffuser face | `ceiling_light_round_01.png` | **1:1** | 1024×1024 (current production); 256×256 painter output is contract-valid | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius (0.16 m) | POT both edges |
| Decal sheet | `no_diving_01.png` | **asset-defined**; placement must match it | 128×128 small markings; 1024×1024 hero signage | none enforced | **required cut-out**: alpha 0 background. A catalog decal may add `"alpha_mode": "blend"` for a soft-edged feather sheet (path-to-grass strips); the default stays the hard 0.5 cut-out | no | full sheet fitted to the level placement's width × height | POT both edges |
| Prop / entity texture | embedded in `chair.glb` | **model-defined** (shipped 1:1) | 256×256 native (the normal shipped size); 32/64/128 legal for lighter props | none enforced | opaque by default; a glTF material may declare `alphaMode: "MASK"` (+`alphaCutoff`, default 0.5) for alpha-cutout foliage, which draws through the engine's cutout pass, or `alphaMode: "BLEND"` for a translucent material, which only the character/dynamic translucent routes draw (a blended static architecture placement is an authoring error) | no | model `TEXCOORD_0`, normalized 0..1, clamped | hard 1024 engine limit; uniform resize safe, repack is not |
| Sky sheet | `sky_stars_01.png` | **2:1 equirectangular** | 1024×512 | none enforced | RGB; alpha unused | **yes in u** (the horizon seam); v is a pole-to-pole span, clamped at the poles | sampled by view direction: `u` = yaw, `v` = pitch (`0` straight up). No level placement UVs | POT both edges (`ShippedTextureKind::Sky`); the only non-square tiling-class sheet; catalog `texture` with `"surface": "sky"` |
| Emissive mask | (none shipped) | **any**; must share the albedo's UV frame | ≤512 (High budget) | none enforced | RGB sampled, alpha ignored | follows the albedo | same UV frame as the albedo | dimensions need not equal the albedo; a mask-only texture is exempt from the square-surface dimension test |
| Diagnostic texture | `diagnostic_alt_01.png` | deliberately varied (96×64) | n/a | n/a | deliberately varied | n/a | not used by any shipped level | test artwork only |
| Application icon | `icon.png` | 1:1 | ≤512 | — | RGBA | no | n/a | non-interlaced; asserted by `tests/test_package.py` |
| Documentation capture | `docs/screenshots/01-office.png` | 30:17 (960×544) | 960×544 | — | frame image | no | n/a | docs only |

**Do not read this table as "all textures are 1024×1024."** It is not a size
target; it is a per-class shape contract plus the resolution the current
production assets use. The hard ceiling is the only repository-wide number.

---

## 5. World surface textures

Surface textures are the tiling sheets used by wall, floor and ceiling
materials. They share one UV convention: `tiled_uv(a, b, tile_metres)` returns
`[a / tile_metres, b / tile_metres]` (`src/render.rs::tiled_uv`), where `a`
and `b` are world-space metres along the surface, and the same period applies
to both axes. A material's `tile_metres` is authored in the catalog, validated
to `0.05..=64.0`, and defaults to `2.0`
(`DEFAULT_TILE_METRES`).

This means texel density is set by `tile_metres` and the source resolution
together. A 1024×1024 sheet on a 2 m repeat shows 2 mm per texel; the same
sheet at 256×256 shows 8 mm per texel. The *layout* is unchanged by
resolution.

### 5.1 Standard wall textures

| Property | Value |
|---|---|
| Aspect ratio | **1:1 (square)** — mandatory per shipped-asset policy (`ShippedTextureKind::Surface`); the runtime encodes UVs in world units, so a non-square sheet would stretch its cell |
| Preferred source resolution | 1024×1024 (the shipped office/pool wallpaper and tile) |
| Soft warning threshold | above 256 px on either edge (`PREFERRED_TEXTURE_DIMENSION`); the shipped 1024² art is intentionally over it and High uploads it unchanged |
| Hard maximum | 1024×1024 per edge (`MAX_TEXTURE_DIMENSION`, enforced by the decoder) |
| Power-of-two | not required for surfaces; a square NPOT sheet is policy-legal (`assets::tests`) and the 96×64 diagnostic proves non-square NPOT dimensions decode and upload |
| Tileable | **yes, both axes**; seam-gated |
| Channels | RGB or RGBA; 8-bit output |
| Alpha | ignored unless the material authors `cutout` or `blend` |
| UV scale | world metres ÷ `tile_metres`, both axes |
| Wrapping | `REPEAT` + mipmaps |
| Filtering | the global Texture Filtering setting (Low/Medium/High = trilinear + 4x/8x/16x anisotropic), never per asset |
| Tint | the material's `tint` multiplies the sampled texel |
| Orientation | image top row = top of the wall; image left edge on the viewer's left from the side the face looks into; phase anchored to the wall top |

Replacing a wall texture: keep it square and tileable; keep or raise source
resolution up to 1024; do not change the 1:1 ratio. If the new art is not
seamless, run the seam tool (§13).

### 5.2 Floor textures

Same rules as walls, with two differences:

* the UVs are `(x, z)` in world space (`geometry.rs` floor emission), so an
  authored map-style image reads with north (−Z) at the top;
* the floor material's own `tile_metres`, `tint` and alpha mode apply.

**Do not assume floors and walls are interchangeable.** They share the shape
and tiling contract, but a floor sheet is sampled on the ground plane and a
wall sheet on vertical faces; orientation and the material's response
(shine, specular, reflection) are authored per material, not per file.

### 5.3 Ceiling textures

Same rules again, with `(x, z)` UVs like floors. Ceiling materials in the
shipped catalog author a dimming tint (for example `[0.72, 0.72, 0.70]` for
the office panel), so the source artwork is painted pale/neutral (see §18).

### 5.4 Carpet

Carpet is not a separate material class; it is a floor surface sheet whose
material authors no tint. The repository rule that follows from the current
material system is:

* the source artwork carries the full colour (the completed warm-brown carpet
  albedo), because nothing tints it;
* it must tile seamlessly;
* it must be square.

The render test `test_carpet_png_has_no_metre_checker` additionally asserts
that the shipped carpet has no artificial "metre checker": quadrant means
differ by no more than 4 levels and the pile varies by at least 4 levels.
That test is specific to the shipped office carpet; for new carpet artwork the
practical rule is the same: the texture must not expose its repeat grid as a
obvious pattern.

### 5.5 Wallpaper

Wallpaper is a wall surface sheet. Requirements:

* square, 1024×1024 production size;
* tileable in both directions — the right edge must join the left and the top
  the bottom with no visible wrapped step;
* pale/near-neutral albedo where the material tints it
  (`core:wallpaper_yellow_01` uses tint `[0.85, 0.80, 0.42]`);
* the repeat must read at the material's `tile_metres` (the office wallpaper
  uses seven rows/columns of double chevrons per 2 m repeat, about 28.6 cm per motif).

The stained variant is the same paper with damage; the damage must wrap too.
The chevron artwork preserves the pale cream/beige palette and material tint.
Both sheets are loaded from PNG by `office_art.py`, including forced builds.
The stained sheet was seam-repaired with `--band 24 --residual-band 12
--radius 8 --offset 146`; the clean sheet passed without seam repair.

### 5.6 Tile, concrete, metal, plastic, glass and grille

* **Tile** (pool deck/basin/wall) — square 1024×1024, tileable, opaque. The
  painted grid must be regular at the material's `tile_metres` (deck 1.5 m,
  basin 1.0 m, wall 1.0 m). A tile sheet is still a *surface* sheet: the same
  square/tileable/resolution rules apply.
* **Metal and plastic panels** — square core sheets (1024×1024 in the current
  production set), tileable, opaque, optionally paired with a tangent-space
  normal map through the material's `normal_texture`.
* **Glass** — square RGBA sheets (1024×1024 currently) whose alpha is *used*,
  because their materials author `alpha_mode: "blend"`. They are still
  uploaded `REPEAT` and gated for tiling (the surface pipeline has no special
  alpha case). Preserve the alpha gradient when replacing them; an opaque
  glass sheet would lose the material's designed transparency. The three
  shipped sheets are clear, dirty and tinted; their current alpha ranges are
  roughly 27–34, 53–137 and 116–127 of 255.
* **Grille** — a square RGBA *cut-out* surface sheet (1024×1024 currently):
  the alpha channel is the shape and its material authors
  `alpha_mode: "cutout"`. The transparent regions let the surface behind show
  through. Keep the cut-out silhouette and keep the sheet square and tileable.
* **Trim sheets (baseboard, handrail, threshold)** — ordinary wall/floor
  surface sheets that a level puts on the generic trim pieces (`baseboards[]`,
  `guardrails[]`, `thresholds[]`). They keep the surface contract: 1:1,
  tileable in both axes, opaque, sampled at the material's `tile_metres`. The
  Home set (`home:baseboard_wood_01`, `home:baseboard_white_01`,
  `home:handrail_wood_01`, `home:threshold_wood_01`) uses a 0.4–0.5 m repeat, so
  a 9 cm board shows the top ~18 % of the sheet vertically: paint the grain and
  any tonal banding so it reads in that band, and keep the top and bottom rows
  similar (the sheet still tiles vertically).
* **Concrete and standalone artwork/paintings** — not present as separate
  classes in the repository. The catalog's `core:painting_dull_01` material
  reuses the wallpaper texture. A new concrete or artwork sheet would be
  introduced as an ordinary surface sheet (1:1, tileable if repeated) unless a
  new fitted usage is defined; that decision must be recorded here first.

---

## 6. Light fixture faces

A fixture's mesh is generated from a fixture profile
(`src/lighting/tuning.rs::fixture_profile`); its *visible luminous face* is a
catalog `light` entry whose `model` is a PNG. Fixture sheets are **fitted**:
the whole sheet is mapped once across the face, the UVs never leave `[0, 1]`,
and the renderer uploads them `CLAMP_TO_EDGE` with mipmaps. Nothing about a
fixture face tiles.

The luminous face is **texture-first**: the sheet defines the fixture's visible
colour and appearance, and the face's per-vertex emission is a *neutral*
brightness (the authored `emission` strength, defaulting to the fixture's
intensity) multiplied into the sampled sheet. The placed light's `color` is a
property of the illumination only: it tints what the bake casts into the room
and never repaints the face. The sheet itself is *not* a lightmap and carries
no lighting information; its job is the fixture's appearance (diffuser, lens,
housing trim on the luminous face). There is no separate emissive map for a
fixture face.

A fixture's **housing** (the office panel's frame and body; the round and wall
fixtures' bezel, can and drum) is ordinary body geometry drawn through the
shared untextured white sheet (`core:tex_white_01`, §12.2) with the profile's
flat authored shade. It is deliberately untextured: the housing is
metal/plastic body geometry, not artwork, and a theme that wants patterned
housing would introduce a fitted body sheet the way the luminous face already
is one. The texture-first contract therefore covers
everything a player reads as the fixture's *artwork* — the diffuser, lens or
panel face — and the round Home flush mount in particular draws its whole
visible face, rim line and centre structure from
`environment/home/textures/lights/ceiling_light_round_01.png`.

Fixture faces are **opaque by construction**: the face draws in the opaque
pass and its alpha channel is ignored. A shipped fixture sheet is additionally
required to be fully opaque by
`src/loader/tests.rs::test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog`,
so RGB artwork and an RGBA file whose alpha is everywhere 255 are equivalent
in practice.

### 6.1 Fluorescent ceiling panel

Shipped asset: `core:fluorescent_panel_01` →
`assets/environment/office/textures/lights/fluorescent_panel_01.png`.

| Property | Value |
|---|---|
| Aspect ratio | **2:1, landscape (mandatory)** |
| Geometry mapping | the sheet is fitted to the diffuser aperture: a 1.12 m × 0.56 m rectangle inset inside the fixture's 1.2 × 0.6 m troffer footprint (width along world X, depth along world Z), recessed 0.012 m above the frame's bottom |
| UV layout | `u` spans the 1.12 m aperture width (`u = 0` at min X, `u = 1` at max X); `v` spans the 0.56 m depth (`v = 0` at min Z / the −Z edge, `v = 1` at max Z) |
| Orientation | `v = 0` is the image's top row; the twin tubes run across the panel *width*, i.e. horizontally in the image |
| Current asset | 1024×512 (≈1.09 mm per texel both ways) |
| Preferred source resolution | 1024×512 |
| Higher resolutions | allowed while 2:1 and POT both hold: 128×64 → 256×128 → 512×256 → 1024×512 are the same layout. `src/loader/tests.rs::test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog` pins each family's aspect, POT edges, hard limit and full opacity, so a resolution change within that contract needs no test edit. |
| Hard maximum | 1024 per edge, so 1024×512 is the largest valid 2:1 sheet |
| Transparency | none: no transparent padding, no cut-out, no alpha use |
| Emissive information | embedded in the artwork's brightness only; the glow is added by the renderer, and the base texture always multiplies the emission term |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

The 1.2 × 0.6 m outer footprint is the *housing*: four untextured side walls
drop 0.045 m from the ceiling plane, a bottom frame borders the diffuser on all
four sides, a top flange closes the body and the diffuser sits 0.012 m recessed
above the frame's bottom. Only the fitted sheet glows; the housing is a fixed
mid grey and carries no artwork.

A `90°`/`270°` rotated panel placement is a **known code discrepancy**: the
renderer swaps the face's world X/Z extents but leaves the sheet's UV axes
fixed, so the sheet is effectively rotated and stretched onto a roughly 1:2
aperture (0.52 × 1.16 m) rather than rotated as a sheet (contrary to the comment
in `src/render/common/fixtures.rs`). No test covers rotated-panel UVs. Until that is
resolved, do not treat the rotated placement as an additional contract for the
artwork; keep the sheet 2:1 landscape and flag a rotated fixture for review
(§25.1). An unrotated panel is exactly isotropic: the 1.12 × 0.56 m aperture
shows 1.09 mm per texel in both directions at 1024×512.

The visible lens face is 0.4 × 0.2 m (`src/render/common/fixtures.rs`, pinned by
test). The map guide's separate "0.4 × 0.18 m" figure for the wall luminaire
describes the *bake's light rectangle* (`half_width: 0.20`,
`half_depth: 0.09` in `src/lighting/tuning.rs::fixture_profile_for_kind`), not
the lens artwork; both figures are current and describe different things.

### 6.2 Round recessed downlight (pool ceiling light)

Shipped asset: `core:pool_light_round` →
`assets/environment/pool/textures/lights/pool_light_round_01.png`.

| Property | Value |
|---|---|
| Aspect ratio | **1:1 (mandatory)** |
| Geometry mapping | planar diffuser ring in the fixture's own plane; the sheet centre is the fixture centre and the sheet's inscribed circle is the diffuser's outer radius (0.22 m) |
| UV layout | `u = 0.5 + 0.5·(r/R)·cos θ`, `v = 0.5 + 0.5·(r/R)·sin θ`, with `u` along world X, `v` along world Z |
| Orientation | concentric artwork (rings, a lamp core) lands centred on the fixture; the mapping is isotropic, so one texel covers the same distance on both in-plane axes |
| Current asset | 1024×1024 |
| Preferred source resolution | 1024×1024 |
| Higher resolutions | allowed while 1:1 and POT hold, up to 1024×1024 |
| Transparency | none; the artwork is the diffuser face, not a cut-out |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

Do not copy the fluorescent panel's 2:1 rule here. This face is square and
isotropic by construction.

### 6.3 Wall luminaire (pool wall light)

Shipped asset: `core:pool_light_wall` →
`assets/environment/pool/textures/lights/pool_light_wall_01.png`.

| Property | Value |
|---|---|
| Aspect ratio | **2:1, landscape (mandatory)** |
| Geometry mapping | the lens face is 0.4 m wide × 0.2 m tall, oriented by the placement's `rotation_degrees` around Y |
| UV layout | `u` runs across the face width (0.4 m); `v` runs up the face height (0.2 m), bottom at `v = 0` |
| Current asset | 1024×512 |
| Preferred source resolution | 1024×512 |
| Higher resolutions | allowed while 2:1 and POT hold, up to 1024×512 |
| Transparency | none |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

### 6.4 Residential flush-mount ceiling light

Shipped asset: `home:ceiling_light_round` →
`assets/environment/home/textures/lights/ceiling_light_round_01.png`.

| Property | Value |
|---|---|
| Aspect ratio | **1:1 (mandatory)** |
| Geometry mapping | planar diffuser ring in the fixture's own plane, 0.07 m below the ceiling; the sheet centre is the fixture centre and the sheet's inscribed circle is the diffuser's outer radius (0.16 m) |
| UV layout | `u = 0.5 + 0.5·(r/R)·cos θ`, `v = 0.5 + 0.5·(r/R)·sin θ`, with `u` along world X, `v` along world Z |
| Orientation | concentric artwork (the diffuser tone and its moulded rim) lands centred on the fixture; the mapping is isotropic |
| Current asset | 1024×1024 |
| Preferred source resolution | 1024×1024 |
| Higher resolutions | allowed while 1:1 and POT hold, up to 1024×1024 |
| Transparency | none; the artwork is the diffuser face |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

The artwork is the *whole* visible lamp appearance: the lit ring samples the
sheet's inscribed circle, and the drum, its bottom rim and the centre boss are
untextured body geometry. Do not draw the drum in the sheet, and do not replace
the sheet with a flat colour: a code-tinted face would violate the texture-first
rule every fixture family follows.

### 6.5 Ceiling lights and wall lights, summarised

There is no separate "ceiling light" or "wall light" texture class. A placed
light names a fixture id; the fixture id selects one of the three families and
therefore one of the three face contracts:

| Family | Example id | Face aspect | Placement notes |
|---|---|---|---|
| Fluorescent panel | `core:fluorescent_panel_01` | 2:1 | ceiling troffer: the sheet is fitted to the inset diffuser inside a grey frame; `rotation_degrees` turns the panel |
| Round recessed | `core:pool_light_round` | 1:1 | ceiling downlight |
| Wall luminaire | `core:pool_light_wall` | 2:1 | needs `"mount": "wall"` and a world `"y"` |
| Flush mount | `home:ceiling_light_round` | 1:1 | residential ceiling lamp: a drum with a glowing diffuser disc |

Adding a further fixture family is a code change (a new `FixtureKind`, profile
and geometry in `src/lighting/tuning.rs` and `src/render/common/fixtures.rs`) plus an
asset. Do not add a fixture PNG without adding the family and its face
contract.

---

## 7. Decals and signs

Decals are local surface markings (floor arrows, hazard stripes, safety
signs). Two decal pipelines exist:

* an internal fixed PNG atlas (`core:decal_test_01` only), which is test
  machinery and not an authoring path; and
* **file-backed decal sheets**, the normal authoring path. A catalog `decal`
  entry's `model` is the PNG.

Contract for a file-backed decal sheet:

| Property | Value |
|---|---|
| Aspect ratio | **asset-defined**; the level placement's `width` and `height` (metres) are the quad, and the whole sheet is fitted across it |
| Choosing dimensions | pick `width`/`height` in the level so their ratio equals the sheet's pixel ratio; otherwise the artwork stretches. A square sheet is placed with `width == height`. |
| Preferred source resolution | 128×128 for small markings (arrows, stripes); 1024×1024 for hero signage (the NO DIVING sign ships at exactly 1024×1024) |
| Hard maximum | 1024 per edge |
| Power-of-two | **both edges must be POT** per shipped-asset policy (`ShippedTextureKind::DecalSheet`); the renderer samples decals with mipmaps |
| Channels | RGBA required for a cut-out: the background is alpha 0 |
| Alpha | the decal pass discards texels below 0.5 (`DECAL_ALPHA_CUTOFF`); alpha is the silhouette |
| Tiling | **no** — the sheet is fitted once and never repeats |
| Wrapping | uploaded `REPEAT` with mipmaps (an implementation detail); UVs stay inside `[0, 1]` |
| Orientation | the artwork reads upright and unmirrored in the world, exactly as in an image viewer; `rotation_degrees` spins it in the surface plane |
| Placement | decals lie flat on a floor, ceiling or named wall face, offset from the surface by the shared decal depth bias |

The shipped sign's contract is additionally pinned by tests: it is 1024×1024,
PNG colour type 6 (RGBA), and contains at least one fully transparent pixel.
A replacement must keep those properties.

The **ceiling vent** (`core:decal_ceiling_vent_01`) is a square 128×128
cut-out sheet with a pale bevelled plate, dark recess, five horizontal louvres
and four corner fixings. Keep its square aspect, upright grille orientation
and transparent margin. Place at 0.6 m × 0.6 m; `align: "ceiling_grid"` centres
it within a ceiling panel. The authored PNG is the source of truth;
`tools/textures/decal_art.py::build_ceiling_vent` loads it without repainting.
`python3 tools/textures/build.py --only core:decal_ceiling_vent_01` round-trips
that artwork and validates it.

A new decal sheet does not need to be square, but it does need POT edges, an
alpha cut-out and a level placement whose width/height match its proportions.
If in doubt, author square artwork and place it square.

### 7.1 Steam effect sheets

An **effect sheet** is the artwork of a level's `effects[]` billboards: the
ambient steam plumes (`src/render/common/effects.rs`,
`src/render/wgpu/effects.rs`). It is neither a world surface nor a decal: the
level authors an emitter volume, the renderer generates camera-facing quads,
and the sheet is sampled once per particle.

| Property | Value |
|---|---|
| Aspect ratio | **1:1 (square)** |
| Preferred source resolution | 256×256 (the shipped `steam_01.png`); 128×128 painter output is contract-valid |
| Hard maximum | 1024 per edge, like every PNG |
| Channels | RGBA is effectively required: the alpha channel is the puff |
| Alpha | **required**, and must fall to 0 at every edge. The particle's own fade-in/out envelope multiplies it, and the material's `opacity` multiplies again |
| Tileable | sampled `REPEAT`, and the zero edge alpha is what keeps a repeat invisible; the artwork itself is a radial puff, not a seam-matching pattern |
| UV contract | one billboard per particle; `u`/`v` run `0..size / tile_metres`, where `size` is the effect's authored particle size and `tile_metres` the material's period. The shipped material uses `tile_metres: 0.5` |
| Orientation | a puff is rotationally symmetric, so orientation is not part of the contract; keep the artwork centred and understated |
| Catalog shape | a `core` `texture` entry (`core:tex_steam_01` → `core/textures/effects/steam_01.png`) plus a `source: "definition"` `material` entry (`core:steam_01`) with `alpha_mode: "blend"`, a low `opacity`, no tint, no emission and no reflection |
| Painting path | `tools/textures/extra_art.py::build_steam`, regenerated by `python3 tools/textures/build.py --only core:tex_steam_01`; deterministic helpers only |

An effect billboard is never collidable, never occludes baked light and is not
part of the lighting bake. The RGB channels are sampled as authored and
multiplied by the particle's alpha; keep the puff white-grey and place all of
the shape in the alpha channel.

---

## 8. Model and prop textures

Prop and entity textures are different from world textures in almost every
respect. They are not tiles: they are fitted to a model's own UV map.

### 8.1 Format and location

* Models are self-contained binary glTF 2.0 files (`.glb`), read by
  `src/gltf.rs`. The supported subset is deliberately narrow: triangles only,
  `POSITION`, `TEXCOORD_0` and optional `COLOR_0`, 16/32-bit indices, optional
  skins (`JOINTS_0`/`WEIGHTS_0`, one skin per model) and optional LINEAR/STEP
  animation clips, no morph targets. A skinned model's static prop batch draws
  its bind pose; a placed skinned model is re-posed every frame by the
  character path (see [MAP_AUTHORING_GUIDE.md §16](MAP_AUTHORING_GUIDE.md#16-props-and-models)).
* **Textures are embedded PNG bufferViews inside the GLB.** External images
  and `data:` URIs are rejected with the message "external or data-URI images
  are not supported; embed the PNG in the GLB". A `.png` file next to a model
  is not used at runtime, but remains the editable source/record of its artwork.
  `tools/assets/audit.py` requires an equivalent standalone PNG for each
  embedded image; a shared family source is sufficient.
* The texture must be a PNG, and the parser rejects any embedded image above
  1024 px on either edge. A model with an invalid or oversized texture does
  not load partially: it falls back to its catalogue placeholder box.
* Most shipped models carry one embedded sheet; Spooner-Man carries three.
  The runtime accepts up to 16 images, 16 materials and 32 primitives per
  model, one material per primitive.

### 8.2 UV contract

* Every vertex must carry `TEXCOORD_0`. The parser rejects a primitive
  without it ("primitive has no TEXCOORD_0; every prop vertex must be UV
  mapped").
* UVs must lie within `0..1` (±0.01). Tiling UVs are rejected: "UV lies
  outside 0..1; props use non-tiling UVs". Model textures never repeat.
* The renderer samples model textures `CLAMP_TO_EDGE` with mipmaps.
* UVs are normalized fractions of the image, so **a uniform resize keeps every
  UV pointing at the same relative artwork**. Moving, reordering or
  re-packing regions changes what the UVs sample.

### 8.3 Sheet layout

The shipped toolkit organizes one square canvas per model into named *regions*:

* an `auto` grid (2 regions → 1×2 cells; 3–4 → 2×2; 5–8 → 4 columns) or
  explicit pixel rectangles for hand-packed sheets (Spooner-Man);
* region rectangles become UV fractions at build time and are baked into the
  GLB;
* canvases are square and restricted by the tooling to 32, 64, 128 or 256 px;
  **256 px is the normal native size**, and the refreshed domestic pack ships
  at it.

There is no shared cross-model atlas. Two models may embed byte-identical
sheets (the three pool curtain models share one, as do the three guardrail
models), but each GLB owns its own copy. If a shared family sheet changes, all
members must be rebuilt, or the family visibly drifts.

### 8.4 Resolution policy

The pipeline distinguishes three sizes:

```text
source / master artwork        (optional, e.g. 512x512 or 1024x1024,
        |                       kept beside the model as an authoring source)
        v
asset build pipeline           (tools/props: embeds the native runtime sheet;
        |                       refuses to ship above the native size)
        v
normal runtime texture         256x256 native
```

| Property | Value |
|---|---|
| Aspect ratio | model-defined; shipped models use square sheets, but the loader accepts any shape up to 1024² |
| Native size | **256×256** is the normal shipped prop texture size (`PROP_TEXTURE_NATIVE_SIZE`); 32/64/128 remain legal for lighter props |
| Shipped sizes | 136 embedded sheets across 134 models: 32×32 (1), 64×64 (6), 128×128 (45), 256×256 (84); see `tools/assets/audit.py` for the current inventory |
| Higher resolutions | The engine accepts embedded prop images up to 1024 px per edge (`MAX_PROP_TEXTURE_SIZE`) and downscales them to the active level's budget; no shipped atlas uses more than the native 256 because High never samples a prop sheet above it |
| Hard maximum | 1024 px per edge (parser rejects the model above it) |
| Runtime budget | High and Medium upload prop sheets at ≤256 unchanged; Low at ≤128 (`TextureClass::Prop`); downscaling preserves aspect via one integer factor |
| Pack budget | 64 MiB decoded RGBA8 for the whole shipped pack (`PROP_TEXTURE_PACK_BUDGET_BYTES`); the audited 134-model library decodes to approximately 24 MiB before runtime sharing/downsampling |
| Decoded memory | `width × height × 4` bytes per image (RGBA8), summed over a model for `texture_bytes`; the surface class additionally caps one sheet at `MAX_SURFACE_TEXTURE_BYTES` (4 MiB) |
| Practical guidance | 256×256 is ordinary content, not a special high-quality variant; authoring above 256 only spends GLB bytes that no quality level displays |

### 8.5 Materials, alpha and emissive maps

* The loader reads `pbrMetallicRoughness.baseColorTexture` and
  `baseColorFactor`, `emissiveFactor`, `emissiveTexture`,
  `KHR_materials_emissive_strength`, `alphaMode` and `alphaCutoff`. It ignores
  `metallicFactor`, `roughnessFactor`, `normalTexture`, `occlusionTexture`,
  `metallicRoughnessTexture`, `doubleSided` and samplers.
* **Props draw opaque unless the material declares `alphaMode: "MASK"`.** A
  masked primitive becomes the engine's alpha-tested cutout pass at the
  authored `alphaCutoff` (glTF default 0.5); this is what grass tufts and tree
  leaf cards use. The toolkit writes full opacity into every prop sheet unless
  the builder opts in with `p.set_texture(size, alpha=True)` and
  `p.material(..., alpha_mode="mask")`.
* **`alphaMode: "BLEND"` is the blended-material contract.** The importer reads
  it as a real translucent material and only the routes with a translucent
  pass draw it: skinned characters (a fading sheet-ghost entity) and dynamic
  objects. The static prop batch pass still draws opaque and cut-out batches
  only, so a blended model placed as static architecture is a mistake — author
  it as an entity. A glTF `baseColorFactor` alpha is folded into the vertex
  colours by the importer, so the blend contract itself carries opacity `1.0`;
  per-instance fade is a runtime component, not an asset property.
* An emissive material is one slot of a model and lights nothing by itself
  (`emissiveFactor`/`KHR_materials_emissive_strength`, optional
  `emissiveTexture` mask). The Halloween entities use this for the carved
  candle flame, the sheet ghost's cloth (with its face-feature primitive kept
  much dimmer so the painted face stays readable) and the pumpkin head; their
  actual illumination is the runtime `glow` component, never a baked static
  light.
* A masked prop still bakes as a *solid* occluder unless its level placement
  sets `"occludes": false`: the bake derives coarse boxes from triangles and
  cannot see alpha. Every scatter tool writes `"occludes": false` for foliage.
* `baseColorFactor` is baked into vertex colours; the renderer adds the
  material's emissive term on top, never multiplied by the baked light.
* An `emissiveTexture` is sampled with the *same* `TEXCOORD_0` as the base
  texture and only its RGB is used. It must therefore use the same UV frame as
  the base map.

* The shipped `core:exit_sign` and `home:ball_light` are the first models to
  author `emissiveFactor` (plus `KHR_materials_emissive_strength`) on a
  separate material slot: the green sign face and the orb glow. Their
  *illumination* is authored separately in the level (`props[].lights`),
  because a material never lights a room; disabling the level light leaves the
  glowing face unchanged and removes the pool.
* A model may declare animation clips without a skin (`home:wall_switch`): it
  parses as a rigid animated prop, every primitive is bound to its owning node
  with weight one, and the character path poses it. See
  [MAP_AUTHORING_GUIDE.md section 16](MAP_AUTHORING_GUIDE.md#16-props-and-models).


### 8.6 Replacing a model texture

* **Uniform resize** (e.g. 128×128 → 256×256) keeps the layout and is safe.
  Keep the sheet square unless the model was authored otherwise.
* **Do not re-pack the atlas.** Editing pixels in place is safe; moving
  regions requires regenerating the GLB.
* **Do not exceed 1024 px** on either edge; the whole model fails if you do.
* The repository's regeneration path is the toolkit:
  `python3 tools/props/build.py --only <id>`. It validates UV bounds, scale,
  origin, triangle and texture budgets. The shipped Spooner-Man entity rig is
  hand-authored and maintained by `tools/props/animate_spooner_man.py`; it is
  never rebuilt from the placeable-prop builders.

The refined sign/fixture/CRT builders also load standalone PNG source atlases
beside their GLBs. Preserve their existing region frames: stop sign uses the
2×2 sign/pole/bracket/back layout; the rebuilt exit sign uses face
`(0,0,256,128)`, body `(0,128,128,128)` and metal `(128,128,128,128)` on a
256² sheet. Its 2:1 face is recessed 6 mm into a closed bevelled housing;
only that face uses the emissive material. Globe and switch retain their three-cell 128² layouts; CRT retains
screen/bezel/body/panel on a 128² 2×2 sheet. The mannequin uses one opaque
256² light-grey concrete sheet at `entities/mannequin/textures/concrete_grey_01.png`.
These source files are embedded during export; runtime reads the GLBs.

The domestic remakes in `tools/props/parts/domestic_remade.py` retain those
atlas regions and the tableware layouts. Bed and office chair now load their
existing embedded artwork from standalone 256² PNG sources beside their GLBs:
`core/props/models/bed.png` (frame/mattress/linen/board) and
`environment/office/props/models/chair.png` (shell/pad/metal/dark). These are
unchanged extracted pixels, not newly painted textures. The switch retains its
`toggle` clip and node names; its rocker hinge is now at `(0, 0.066, 0.022)`
metres, with a 0.35-second, 30-degree travel. Its back remains at `z = 0`.

Pool curtain and guardrail remakes load their existing 256² source PNGs beside
all six GLBs in `environment/pool/props/models/`. The curtain atlas preserves
cloth/post/plate/track; the rail atlas preserves post/rail/plate/spare. Their
pixels and 2×2 region layout are unchanged. Rail axes are at 0.98 m and 0.525 m
on every leg. Curtains use closed 2 mm cloth shells with continuous fitted UVs,
visible track tabs and a 7.5 cm minimum floor clearance. Catalog dimensions,
module origins and placement IDs remain unchanged.

The remade rubber duck loads `environment/pool/props/models/rubber_duck.png`,
an opaque 256² atlas retaining body/head/beak/eye in the existing 2×2 layout.
The cells are clean yellow, lighter yellow, orange and charcoal material
swatches; the eye silhouette comes from small closed mesh discs, not alpha.
The atlas is embedded by `tools/props/parts/duck_remade.py` during export.

### 8.7 Winter static snow kit

Winter uses the existing surface/model classes, with no new shader or raster
contract. `winter:tex_snow_01` is an opaque, seamless 1024² floor sheet at
`environment/winter/textures/floors/snow_01.png`. Snow and packed snow share
this source; their world-aligned repeats are 8 m and 5 m respectively. The
packed material uses a restrained compaction tint. Both are matte and carry
no normal map, emission or baked directional illumination.

`environment/winter/props/models/snow_surface.png` is its native 256² fitted
derivative. Modular drifts, architecture caps and icicles embed that PNG;
their top UVs are continuous across neighbouring facets. All snow/ice shells
are closed and outward-wound, with no blend materials.

`tools/props/parts/winter.py` imports the committed canonical evergreen,
rocks and railing meshes verbatim: original vertex positions, indices, UVs,
colours, atlas layout and MASK foliage remain unchanged. It adds a separate
`snow_accumulation` mesh/material. This material samples the existing committed
white sheet through the supported untextured-material path; the original
embedded atlas and equivalent outdoor standalone PNG stay intact. No snow
artwork is painted at build/load time. The exporter accepts
`material(..., use_texture=False)` for that existing runtime contract.

The two evergreen loads are 1238 and 1442 triangles, explicitly reviewed above
800 because each retains all 770 original triangles plus closed supported
snow. Every other winter model is below 500 triangles. Tree dimensions remain
3.2 × 6.8 × 3.2 m; the level retains the original narrow trunk collider.
Snow on rocks and rails increases their visual envelope only. Their collision
sizes must remain those of the canonical bare structures.

Modular cap origins are their base-contact plane, centred in X/Z. Sink caps
8–9 mm into their supports to avoid coplanar seams. Icicle origins are the
lowest tip, with all roots at the catalogue height; mount the root plane
15 mm into the underside. Prop Y remains floor-relative. See
`assets/environment/winter/README.md` for the module dimensions and roof pitch.
`python3 -m unittest tests.test_winter_assets` protects canonical identity,
supported snow, UVs, winding, sealed topology, budgets and reproducible exports.

### 8.8 Model geometry conventions (for context)

* 1 model unit = 1 metre; +Y up; +Z is the model's front at
  `rotation_degrees = 0`.
* An **entity frame** (the per-frame handoff to the character renderer) carries
  its world yaw in **radians**, `0` facing world `+Z`, the same unit as the
  route state and the character pose; the AI layer stores yaw in degrees and
  converts once at that handoff. See `assets/entities/README.md`.
* The origin is the floor-contact point, horizontally centred under the
  model's bounding box (base at `y = 0`). A **hovering skinned character** is
  the one exception: its bind pose may float above the origin (the sheet ghost
  starts at ~0.17 m), and the level places it at `y = 0` so the hover is the
  asset's own. The horizontal centre and a non-sinking base are still enforced.
* The model's bounding box must match the catalog `size` within
  `max(2 cm, 6 % of the axis)`; the shipped-asset test and the prop tooling
  enforce it.
* Budgets: 500 triangles preferred, 800 needs justification, 1500 is the prop
  art budget; a **skinned character** gets 3000 (the shipped pumpkin-head
  skeleton is 2,278 across 94 joints), and the engine loads up to 6000, then
  falls back to a placeholder. Models above the review threshold are listed in
  the shipped-asset allowlist with their justification.

The outdoor containment peg (`outdoor:collision_peg`) is the one model whose
drawn geometry is meant to be hidden: a deliberately tiny 6 cm opaque cube with
a 32×32 sheet, shipped only as the carrier for a level-authored invisible
containment collider. A level places it `solid: true` with its own collider
`size`, sets `occludes: false` and authors a `y` below the local floor, so the
bake never sees the cube and the level contributes collision and nothing else.

---

## 9. Emissive textures and masks

Places does **not** use dedicated emissive colour textures. Emission is
composed in the shader as:

```
emission = mix(material_emissive_color, vertex_color, vertex_emission)
           × mask × base_texture.rgb × emission_scale
```

* **Material emission** comes from the catalog: `emissive` (RGB) plus
  `emissive_intensity`, resolved as `emissive × intensity` with intensity
  capped at 8. A material with no `emissive` entry is not emissive.
* **The base texture always multiplies the emission.** A dark pixel in the
  albedo cannot glow. Artwork therefore shapes the glow even with no mask.
* **An optional mask** is a separate PNG named by the material's
  `emissive_mask` field. Its RGB selects where emission applies; alpha is
  ignored. The mask is sampled with the same UVs as the albedo, so it shares
  the same world-space UV frame and the same tiling period. **It does not need
  to match the albedo's pixel dimensions**, and the implementation never
  compares them.
  * If you do author a mask, keep it in the same UV frame, orientation and
    tiling period as the albedo, or the glow will not line up with the
    artwork.
  * Masks upload like a surface-class texture but with their own budgets:
    High ≤512, Medium ≤256, Low ≤128.
  * A texture used *only* as a mask is exempt from the shipped-asset dimension
    test, because the engine imposes no shape on it. A sheet shared as both an
    albedo (or normal map) and a mask must satisfy the surface contract.
  * A mask that cannot be read or decoded is not a partial failure: the whole
    material degrades to the missing-texture diagnostic and its emission,
    response and alpha settings are cleared.
  * No shipped material currently authors an `emissive_mask`; the feature is
    covered by unit tests only.
* **Fixture faces** use the vertex-emission path: the whole luminous face is
  emissive and the per-vertex colour scales the sheet. There is no emissive
  mask for fixtures.
* **Emissive animation** (pulse/flicker) is authored per level in
  `animated_emissions` and scales only the emissive term.

If an emissive map is added for a material, state its dimension relationship
to the base texture in this document as part of the same change.

---

## 10. Normal maps

* A normal map is an ordinary PNG in the asset tree that a material names with
  `normal_texture` (plus `normal_strength`, `0.0..=2.0`).
* It is a **surface-class** texture for quality purposes: High 1024, Medium 512, Low 256.
* It is square and tileable, exactly like the albedo it augments, and is
  sampled with the same world-space UVs and the same `tile_metres` period.
* RGB carries the tangent-space normal (`0..255` maps to `-1..1`); alpha is
  unused and shipped normal maps are fully opaque.
* The current production normal maps are 1024×1024 core sheets (the
  deterministic painters still emit 128×128 output); the contract is square
  and tileable at any accepted size up to 1024.
* The environment-class seam gate includes normal maps.

---

## 11. Level-pack textures

Level packs are not supported. A level is compiled into a `.placesmap` package,
and every texture it uses is a catalogued PNG under `assets/` (see §2). A
`materials.json` mapping `pack:` ids and pack-local `textures/*.png` files have
no producer in the current workflow: the loader and compiler resolve materials
through the catalog only, so `pack:` material and fixture ids do not resolve.

A community map ships custom artwork by registering it in `assets/catalog.json`
like any other texture or prop. Self-contained packages with embedded texture
payloads are part of the package format (`kind: "embedded"` dependencies), but
the current compiler builds maps against the installed asset bundle; see
[PACKAGE_FORMAT.md](PACKAGE_FORMAT.md).

---

## 12. Non-runtime imagery

### 12.1 Application icon and documentation screenshots

* `icon.png`: PNG, square, RGBA, non-interlaced, ≤512 px; asserted by
  `tests/test_package.py`.
* `docs/screenshots/*.png`: 960×544 documentation captures. Not runtime
  assets; no contract beyond being still frames.

### 12.2 Shared untextured white sheet

`core:tex_white_01` → `assets/core/textures/white_01.png` is the renderer's
neutral fallback sheet: a flat opaque white fill. Fixture housings, plain
body geometry and every texture slot with nothing better to bind sample it
(§6). It carries no artwork, no level references it, and nothing derives
detail from it, so its size is a budget choice rather than a UV or aspect
contract; it must stay an opaque white fill. The shipped sheet is authored at
the 1024 hard budget like the upgraded surface set and carries sub-1%
dither (every channel stays within 252..=255), which reads as flat white.

It is an ordinary committed catalog PNG (`asset_class: "core"`), resolved
through `assets/catalog.json` like any other texture and loaded once at
renderer startup. The catalog entry, not a hard-coded pixel array, is the
source of truth. It is uploaded `CLAMP_TO_EDGE` with nearest filtering and no
mipmaps, because every sample reads the same white texel. It is not a surface
material and is exempt from the tiling and environment-seam gates for that
reason; its squareness and dimension budget are still checked by the shared
shipped-sheet tests.

A theme that wants patterned fixture housing would introduce a fitted body
sheet the way the luminous face already is one — the white sheet itself is not
an authoring target.

### 12.3 Internal image resources

The fixed internal sheets are ordinary catalogued PNGs. They are embedded
from their repository files so a broken external asset or an installation
without an asset directory can still display the UI and diagnostic fallback.
Their decoded RGBA pixels are pinned by tests against the previous runtime
images; changing compression is safe, but changing layout or orientation is not.

| Image | Catalog id and repository path | Contract |
|---|---|---|
| 128×64 HUD font atlas | `core:tex_ui_font_01` → `assets/core/ui/font_01.png` | Fixed 2:1 RGBA8 sheet, top-down rows; ASCII 32–126 in sixteen 8×8 cells per row. Cell zero stays opaque white for UI quads, glyph ink is white with binary alpha, unused cells remain transparent. No tiling. |
| 256×256 validation decal atlas | `core:decal_test_01` → `assets/core/decals/validation_atlas_01.png` | Fixed square RGBA8 sheet; four 128×128 cells, eight-pixel UV gutters, only slot zero contains the existing frame and text. The PNG retains the atlas's bottom-up row convention, so its text appears vertically inverted in an ordinary image viewer. Preserve that orientation and alpha-zero background; the existing atlas UVs make it upright in game. No tiling. |
| 64×64 missing-texture checker | `core:tex_missing` → `assets/core/textures/missing_01.png` | Fixed square opaque RGBA8 sheet; eight-pixel cells alternate `[255, 0, 255, 255]` and `[24, 24, 24, 255]`, starting magenta at the top-left. Preserve its symmetric orientation and checker period. |
| 2×2 emergency white sheet | `core:tex_white_fallback_01` → `assets/core/textures/white_fallback_01.png` | Fixed square opaque RGBA8 sheet, every channel 255. Used only if the embedded primary white sheet cannot decode. No tiling or authored UV detail. |

These layouts are engine resources rather than map authoring targets. The
existing atlas accessors and decal material slot remain unchanged. The shared
decode accepts only a closed set of embedded repository files; failure is a
repository build invariant, covered by complete pixel hashes and dimension
checks. External images continue to use fallible decoding and the checker.

Computed lighting images are derived from level geometry and light settings:

| Image | Producer | Purpose |
|---|---|---|
| Lightmap atlas pages | `src/lighting/lightmap/` | baked light data, regenerated at level load; never a shipped asset. A developer path can dump a page as a PNG under `target/`, but that is a diagnostic capture, not an asset. |
| Reflection probe cubemaps | `src/render/common/reflections.rs` (routing) and `src/render/wgpu/reflections.rs` (probe cubemaps) | baked per level load |

Diagnostic textures under `assets/diagnostic/textures/` are real PNGs but are
engine test artwork: the 96×64 sheet deliberately proves arbitrary NPOT
dimensions decode and the alpha sheet deliberately proves alpha decode. They
are excluded from shipped levels and from the dimension-contract test below.

---

## 13. Tiling requirements

**Tileable classes** (must join right→left and top→bottom):

* every environment surface sheet (walls, floors, ceilings);
* the core surface sheets used as surfaces: glass, linoleum, metal, plastic
  panels, grille and the normal maps.

**Non-tileable classes** (never gated, never expected to wrap):

* fixture faces (fitted, `CLAMP_TO_EDGE`);
* decal sheets (fitted cut-out);
* prop and entity textures (model UVs, `CLAMP_TO_EDGE`);
* icon, screenshots and diagnostic sheets.

Measurement, when needed:

```sh
# Metrics per axis and channel (always exits 0)
python3 tools/textures/seam_repair.py --report <path.png>

# Gate: exits non-zero on any failure
python3 tools/textures/seam_repair.py --check <path.png> [<path.png> ...]

# Deterministic repair (rewrites the file; parameters for repaired
# shipped sheets are pinned in the tool)
python3 tools/textures/seam_repair.py --repair <path.png>
```

Acceptance for both the raw and the three-tap-smoothed profiles, on both axes,
for every measured channel (R, G, B where present, and luminance):

```
mean(wrap) <= 1.60 × mean(interior) + 1.0
p95(wrap)  <= 2.20 × p95(interior)  + 3.0
```

Wrapped edge is measured exhaustively; the interior reference is sampled every
4th line and every 8th position. Alpha is never measured. The Rust render test
`test_shipped_surface_textures_tile` independently checks the three-tap
smoothed profile for ten named office/pool surfaces (RGB channels only, no
luminance); the Python tool checks both profiles for all nineteen catalog
environment sheets. A sheet is expected to pass whichever gates cover it.

Repository gates that run the seam tool automatically:

* `python3 -m unittest tests.test_package` runs `--check` over every
  environment-class texture in the catalog (the six office sheets, four pool
  sheets, three glass sheets, linoleum, metal, plastic, grille and the two
  normal maps).
* `cargo test` runs the Rust tile test over ten named surfaces.

`tools/textures/build.py --check` does **not** measure tileability; it only
checks existence, PNG structure and dimensions.

---

## 14. Resolution policy

### 14.1 Source versus runtime

The source PNG is not necessarily what reaches the GPU. The quality level
(`settings.json` → `"quality": "low" | "medium" | "high"`, default `high`) sets
a runtime edge budget per texture class:

| Texture class | High (default) | Medium | Low |
|---|---|---|---|
| Surface sheet | 1024 | 512 | 256 |
| Fixture face | 1024 | 512 | 256 |
| Decal sheet | 1024 | 512 | 256 |
| Prop sheet (embedded) | 256 | 256 | 128 |
| Emissive mask | 512 | 256 | 128 |
| Lightmap atlas page | 1024 @ 16 texels/m | 1024 @ 12 texels/m | 512 @ 10 texels/m |

* Downscaling happens **once, at upload / level-load time**, through an
  integer-factor box filter that applies the same factor to both edges, so
  aspect ratio is preserved (up to per-edge rounding).
* The result is cached with the texture; nothing is rescaled per frame.
* **High is the native presentation.** A 256×256 prop sheet and a 1024×1024
  surface upload unchanged; High never resamples an asset that already sits
  within its class budget.
* Medium and Low are optional quality/performance reductions, not a hardware
  requirement: Medium halves the sheets (1024 → 512) and keeps native prop
  sheets, Low quarters the sheets and halves the native prop sheet
  (256 → 128). They must never dictate the size of the asset stored in the
  repository.
* Every level uses the same assets, ids, levels and geometry. A lower level is
  not a second art library. **Never author a separate low-resolution asset
  set.**
* No image class bypasses the budget. The font atlas, the decals' internal
  atlas and the lightmap pages are internal machinery, not shipped textures;
  the shared white sheet (`core:tex_white_01`, §12.2) is a catalog texture
  loaded once at startup as a single un-mipped level, so its resident storage
  is the sheet's own size (4 MiB at 1024×1024) and it is level-independent
  by design.

### 14.2 Why sources are kept large

The shipped environment surfaces are 1024×1024 on purpose. High is the
default presentation and uploads them unchanged; keeping the source at the
hard limit means the same file still serves future renderer improvements and
any higher-resolution presentation without re-authoring. Medium derives its
512 px image and Low its 256 px image from the same source. Replacing a
1024×1024 sheet with a 256×256 sheet would visibly lower High quality.

Prop sheets are the opposite case: the native size is 256×256, and High and
Medium upload it unchanged. The engine can still read a third-party GLB with
images up to 1024 px, but it downscales them to the level's budget, so the toolkit
refuses to *ship* embedded prop art above the native size rather than spend GLB
bytes on pixels no quality level displays. Larger master artwork may be kept beside
the model (like `table.png`, which is the 256×256 source of `table.glb`) for
future quality work; the hard limit exists for correctness, not as a target.

### 14.3 Hard limits

* **1024 px per edge** for every PNG the decoder loads: surfaces, fixtures,
  decals, packs and GLB-embedded images. `decode_png` rejects anything larger
  with `texture dimensions {w}x{h} exceed the 1024x1024 limit`.
* A GLB-embedded image above 1024 additionally fails the whole model at parse
  time, and the prop falls back to its placeholder box.
* **4 MiB decoded RGBA8 per surface sheet.** One 1024×1024 sheet is exactly at
  this budget, so in practice it is implied by the edge limit.
* Filtering: the renderer generates mipmaps for all 2D tiling and fitted
  textures. Power-of-two edges are required for fitted sheets (fixture faces
  and decal sheets) so their mip chains stay exact; they are fitted rather than
  tiled. Surfaces tile as square cells and may be square NPOT.

### 14.4 Preferred versus mandatory resolution (per class)

| Class | Mandatory | Preferred | Hard max |
|---|---|---|---|
| Environment surface | square | 1024×1024 (shipped); tooling warns above 256 | 1024 |
| Core surface | square | 1024×1024 (current); 128×128 painter output is contract-valid | 1024 |
| Normal map | square, tileable | 1024×1024 (current); 128×128 painter output is contract-valid | 1024 |
| Fluorescent panel | 2:1, POT | 1024×512 (current production) | 1024 |
| Round downlight | 1:1, POT | 1024×1024 (current production) | 1024 |
| Wall luminaire | 2:1, POT | 1024×512 (current production) | 1024 |
| Decal sheet | POT both edges, cut-out alpha | 128×128 small; 1024×1024 hero | 1024 |
| Prop texture | model UV layout, no tiling | 256×256 native (the normal shipped size) | 1024 |
| Emissive mask | same UV frame as albedo | ≤512 | 1024 |

**No minimum source resolution is enforced anywhere.** The soft "preferred
256" value is an *upper* warning threshold, not a floor. If a class's practical
floor matters (for example, a 1024² sheet downscaled to 256 at Low is the
smallest image most surfaces will ever display), treat that as art direction,
not as an engine rule.

---

## 15. Mandatory, preferred, minimum and runtime-derived

Use this vocabulary when describing an asset contract:

**Mandatory** — required for correct rendering; violating it produces a
stretched, mirrored, clipped, invisible or rejected asset.

* PNG format; signature-checked.
* ≤1024 px on either edge.
* Surfaces: exactly 1:1.
* Fixture faces: 2:1 (panel, wall luminaire) or 1:1 (round downlight), both
  edges POT, orientations as specified in §6.
* Decal sheets: both edges POT; alpha cut-out; placement ratio matching.
* Prop textures: `TEXCOORD_0` UVs inside 0..1; model-defined layout
  preserved; embedded PNG.
* Emissive masks: same UV frame as the albedo.
* Tileable sheets: seamless wrapped edges.

**Preferred** — the current production choice; deviating is allowed but
should be a deliberate decision.

* Environment surfaces: 1024×1024.
* Core sheets: 1024×1024 in the current production set; the deterministic
  painters emit 128×128, which remains contract-valid.
* Prop sheets: the native 256×256; 32/64/128 stay legal for lighter props.
* Decals: 128×128 small markings, 1024×1024 hero signage.
* Fixture faces: the current shipped sizes (1024×512, 128×128, 128×64).

**Minimum** — nothing in the repository enforces a minimum source resolution.
The nearest thing to a floor is a consequence of the runtime budgets: art at
or below the active level's budget is displayed at its own size under that
level, so there is no reason to author below the class's High budget unless
the style wants it.

**Runtime-derived** — dimensions the engine reads rather than assumes.

* Decal quad proportions come from the level placement's `width`/`height`.
* Prop texture use comes from the model's UVs; the engine reads the decoded
  image's actual width/height.
* Emissive mask dimensions are never compared to anything.

The renderer does **not** derive surface texel density from the sheet: it
always maps `tile_metres` metres to one full sheet, so surface sheets are
warped to the material's tiling regardless of their pixel dimensions. This is
why a surface sheet must be square.

---

## 16. Formats and colour

| Property | Value |
|---|---|
| Accepted format | PNG only, verified by the 8-byte signature |
| Accepted PNG colour types | grayscale, grayscale+alpha, RGB, RGBA, palette (with/without `tRNS`). All normalise to 8-bit RGBA. |
| Bit depth | 8- and 16-bit accepted; 16-bit samples are stripped to 8 bits. Output is always 8-bit. |
| Interlaced PNGs | handled by the `png` crate decode path; not exercised by repository tests |
| Animated images | not supported; an APNG would decode as its first frame at best |
| Grayscale | supported (expanded to RGB with alpha 255) |
| Indexed/palette | supported (the decoder expands the palette) |
| Colour space | **no gamma or ICC handling.** No `GL_SRGB` upload, no transfer function, no gamma chunk written or read. Sampled texels are combined in display space. |
| Premultiplied alpha | not used; alpha is treated as straight coverage |
| Metadata | ignored (non-pixel chunks are not read; the decoder keeps only pixels) |

Authoring consequences:

* Keep values in the display-space range the material system expects; the
  material tint and baked lighting multiply the texel and there is no gamma
  step to recover from an over-bright or over-dark authoring pass.
* RGB is sufficient for any sheet whose material is `opaque`: the decoder
  expands it with alpha 255. RGBA is only needed where alpha is actually read
  (`cutout`, `blend`, decal cut-outs).
* The shipped set is mixed: office surfaces are RGB, pool surfaces are RGBA
  with an unused alpha channel, core sheets are RGBA. All are valid. The
  channel choice is not a contract.

---

## 17. Texture filtering and wrapping

Filtering and wrapping are chosen by the **texture's role**, not by the asset:

| Role | Wrap | Mipmaps | Filtering |
|---|---|---|---|
| Surface albedo, normal map, emissive mask | `REPEAT` | yes | global Texture Filtering setting: Low/Medium/High = trilinear + 4x/8x/16x anisotropic |
| External decal sheet | `REPEAT` | yes | global setting |
| Generated decal atlas | `REPEAT` | yes | global setting |
| Fixture face | `CLAMP_TO_EDGE` | yes | global setting |
| Prop/entity texture | `CLAMP_TO_EDGE` | yes | global setting |
| White fallback sheet (`core:tex_white_01`) | `CLAMP_TO_EDGE` | no | nearest |
| HUD font atlas | `CLAMP_TO_EDGE` | no | nearest |
| Lightmap atlas page | `CLAMP_TO_EDGE` | no | linear |
| Reflection probe cubemap | `CLAMP_TO_EDGE` | no | linear |

Consequences for artists:

* The user's Texture Filtering setting is global; you cannot give one asset a
  different filter mode. Every option is trilinear with anisotropic filtering
  (Low/Medium/High request 4x/8x/16x), and an adapter without anisotropic
  filtering keeps the same trilinear filtering at 1x. Author detail that
  survives the filter and the runtime budget. Fine 1-px detail in a 1024²
  sheet disappears at Low (256 px); keep important features at a scale that
  still reads.
* You cannot request a different wrapping mode from the catalog. If an asset
  needs `CLAMP`, it must belong to a fitted class (fixture, decal, prop).
* Mipmaps mean an asset's lowest levels matter: a decal or prop whose
  background must be transparent needs alpha 0 well outside the silhouette,
  or mip bleeding can tint edges. Decal and fixture sheets are POT so their
  mip chain is exact.

---

## 18. Material tinting and baked lighting

The default surface shading is a multiply chain in display space:

```
lit = texture.rgb × vertex_color.rgb × light
vertex_color = material tint × face shade
```

* `light` is the baked lightmap texel (or the vertex-lit fallback).
* The **bake never samples the albedo**; it stores lighting only. The albedo
  is multiplied in at draw time.
* Emission is **added**, not multiplied into the bake:
  `color = lit + sheen + reflection + emission`.

Authoring consequences:

* Where a material authors a tint, paint the source **pale and near-neutral**
  so `texture × tint × light` lands in range. The office wallpaper, panels and
  ceiling are authored this way.
* Where a material authors no tint, the source carries the full colour (the
  carpet is the reference case).
* Because there is no gamma handling, do not compensate for an sRGB workflow
  when painting; author the values as they should appear.
* A surface's brightness in a dark room comes from the lightmap, not from
  baking light into the albedo. Do not paint illumination into a surface
  texture; the same sheet is reused in differently lit rooms.

---

## 19. Orientation rules

These conventions are implemented in the UV generators; preserve them when
replacing artwork.

| Surface | Rule |
|---|---|
| Uploaded PNG | row 0 of the PNG is `v = 0`; the engine never flips an image. `v = 0` is therefore the image's **top row**. |
| Walls | image top row at the top of the wall; image left edge on the viewer's left from the side the face looks into; `u` runs along the wall length (sign-flipped per face so artwork reads unmirrored); `v` is measured downward from the wall top, so the tiling phase is anchored to the face top. |
| Floors and ceilings | image `x` maps to world **+X**, image `y` maps to world **+Z**; an image authored map-style reads with north (−Z) at the top. |
| Glass panes | local `u` from the wall length origin, `v` increasing upward from the sill. This differs from the wall convention (world `u`, downward `v`); see §25.2. |
| Fixture panel | `u` along +X, `v` along +Z, `v = 0` at the min-Z edge; pinned by test. |
| Round diffuser | planar in the fixture plane, centre at `(0.5, 0.5)`, `u` along +X, `v` along +Z; pinned by test. |
| Wall luminaire | `u` across the face width along the rotation's right vector, `v` upward. |
| Decals | upright and unmirrored as in an image viewer; in-plane rotation from the level. |
| Props | model-defined: `+Z` is the front at rotation 0; the UV layout is whatever the model's `TEXCOORD_0` says. |

For a fixture or decal sheet, "which way is up" is therefore directly
observable: the top of the PNG is the top of the face as defined in §6 and §7.

---

## 20. Aspect-ratio changes

**Do not change an existing asset's aspect ratio merely to increase its
quality.**

If the fluorescent panel has a 2:1 contract, a quality increase is:

```
256×128 → 512×256 → 1024×512
```

not:

```
256×128 → 1024×1024
```

Changing an aspect ratio requires reviewing and updating every consumer:

* the UV coordinates (fixture geometry or decal placement);
* the geometry that frames the face (fixture profile constants);
* the material or catalog entry if the sheet's role changes;
* the tests that pin the ratio or the layout;
* this specification.

Raising resolution *within* the ratio is the safe, expected operation. A
1:1 asset can go 128×128 → 256×256 → 512×512 → 1024×1024; a 2:1 asset can go
128×64 → 256×128 → 512×256 → 1024×512. Any of those is a "better source", not
a new contract.

---

## 21. Asset replacement checklist

Before replacing an existing asset:

1. **Identify its asset class** from §4 and the catalog entry.
2. **Check the required aspect ratio** (1:1, 2:1, model-defined or
   asset-defined). Never infer it from the current PNG only.
3. **Check whether the UV layout must stay unchanged** (all fitted sheets:
   fixtures, decals, props).
4. **Check the preferred and hard source resolution** for the class.
5. **Check alpha requirements**: opaque, cut-out (alpha 0 background), or
   blend (alpha gradient). Keep the same channel behaviour.
6. **Check whether it must tile** — and if so, verify the wrapped edges.
7. **Check for paired maps**: a normal map that shares the albedo's UV frame,
   or an emissive mask that must share it.
8. **Check the orientation** for the class (§19).
9. **Check the directory and file name** (§3): keep the path referenced by
   the catalog, and follow `_01` naming.
10. **Run the relevant validation** (§26) and visually inspect the result.

For embedded model textures additionally: keep the sheet's region layout
(model UVs), keep it within 1024 px, and rebuild the GLB through
`tools/props/build.py` if the embedded PNG changes.

---

## 22. New asset checklist

When introducing a completely new asset class (not just a new file in an
existing class), decide and record all of the following in this document:

- [ ] Aspect-ratio contract (fixed ratio, model-defined or asset-defined);
- [ ] Source resolution policy (preferred size, hard maximum, any minimum);
- [ ] UV behaviour (world-tiled, fitted once, model UVs);
- [ ] Tiling requirement (both axes, none);
- [ ] Alpha behaviour (opaque, cut-out, blend);
- [ ] Filtering and wrapping (which role it follows);
- [ ] Material behaviour (tint, shine, specular, reflection, normal map);
- [ ] Emissive behaviour (material emission, vertex emission, mask, none);
- [ ] Validation (what will check it, and the command);
- [ ] Directory and naming convention;
- [ ] What existing class it most resembles, and why a new one is warranted.

Do not let undocumented asset classes accumulate. A new fixture family, for
example, is not "just a PNG": it needs a `FixtureKind`, a profile, geometry
and a face contract in this document.

---

## 23. AI-agent rule

> Before generating, replacing, resizing or converting a production visual
> asset, identify its asset class in `docs/ASSET_SPECIFICATION.md` and preserve
> every mandatory part of its contract.

Specifically, an AI agent must not:

* make every asset square or 1024×1024 because that looks "high quality";
* change a 2:1 fixture sheet into a 1:1 sheet;
* add transparency to an opaque surface or fixture face;
* repack or reorder a model texture atlas;
* mirror, rotate or crop a fitted sheet;
* introduce a seam into a tiling sheet;
* exceed 1024 px on either edge;
* invent an aspect ratio for a fixture from the current file's dimensions.

If no class covers the asset, **inspect the implementation and update this
specification before introducing the new asset.**

---

## 24. Automated enforcement

What exists today (all manual; there is no CI configuration in the
repository):

| Rule | Enforced by | Command | Fails? |
|---|---|---|---|
| Catalog is valid, references resolve, ids unique, level references valid | `tools/assets/validate.py`; runtime catalog parser | `python3 tools/assets/validate.py` | yes (exit 1) |
| PNG exists, real PNG, non-zero, ≤1024 | `tools/textures/build.py --check` | `python3 tools/textures/build.py --check` | yes (exit 1) |
| Over-preferred (>256) warning; non-POT warning | `tools/textures/build.py --check` | same | no (warnings only) |
| Environment surface seams | `tools/textures/seam_repair.py --check` via `tests/test_package.py` | `python3 -m unittest tests.test_package` | yes |
| Ten named surfaces' seams (independent metric) | Rust test `test_shipped_surface_textures_tile` | `cargo test` | yes |
| Six office sheets: square, opaque, ≤1024 | Rust test `test_shipped_texture_assets_are_opaque_and_within_budget` | `cargo test` | yes |
| **Every catalogued file-backed texture/decal/light sheet satisfies its class dimension contract (square surfaces, POT fitted sheets, hard limit)** | Rust test `every_shipped_sheet_satisfies_its_texture_kind_contract` (see below); props/entities are covered by the props tests and the GLB parser, not this test | `cargo test` | yes |
| Shared white sheet: committed PNG, legal square POT size within the hard limit, opaque near-white, and what the renderer actually loads | Rust tests `assets::tests::the_shared_white_sheet_is_a_committed_opaque_white_png`, `render::wgpu::texture::tests::the_fallback_is_the_committed_white_sheet` and `the_embedded_fallback_is_the_committed_asset` | `cargo test` | yes |
| Fixture sheets: family aspect, POT edges, hard limit and full opacity | Rust test `loader::tests::test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog` | `cargo test` | yes |
| Fixture faces: POT, unique sheet, ≤1024 | `tests/test_package.py` | `python3 -m unittest tests.test_package` | yes |
| NO DIVING sign: 1024², RGBA, transparent pixel | `tests/test_package.py` | same | yes |
| Prop GLB: container parses, one mesh, `TEXCOORD_0` present | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) for a missing or unparseable model; budgets, UV range and scale/origin are not evaluated here |
| Prop GLB: UVs 0..1, triangle/vertex/texture budgets, scale/origin, PNG ≤1024 | Rust `props::tests`; runtime parser | `cargo test` | yes |
| Prop art budgets (triangles, native 256 px texture, 64 MiB decoded pack budget) | Rust `props::tests::shipped_prop_assets_match_the_catalogue_and_budgets` and its policy tests | `cargo test` | yes |
| Prop GLB decoded texture memory (per-texture and pack total) | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) |
| Model textures: embedded PNG only, ≤1024, UV bounds | `src/gltf.rs` parser (runtime) + `cargo test` | game run / `cargo test` | fallback box / test failure |

The strengthened Rust test added with this specification
(`src/assets/tests.rs::every_shipped_sheet_satisfies_its_texture_kind_contract`)
walks every file-backed `texture`, `decal` and `light` entry in the catalog,
decodes its PNG and checks the dimensions against
`ShippedTextureKind::Surface` / `DecalSheet` / `FixtureFace`. Diagnostic
entries are skipped because the 96×64 sheet is a deliberate NPOT probe, and a
texture used only as an emissive mask is skipped because the engine imposes no
shape on a mask. Its failure messages name the asset, the path and the
violated rule, for example:

```
core:tex_pool_tile_wall_01: `environment/pool/textures/walls/pool_tile_wall_01.png`:
a surface sheet must be square, found 1024x2048
```

Known enforcement gaps (checked here, not automated):

* no minimum source resolution for any class;
* pool surface squareness and opacity (squareness is covered by the
  surface-squareness test; opacity is not asserted for pool sheets, though they
  are opaque in fact);
* decal non-POT is only a tooling warning for the small sheets (the Rust
  test makes it a test failure for shipped decals);
* `tile_metres` is not compared against the painted repeat period;
* surface and decal orientation, and the "pale albedo" convention, are not
  machine-checked (fixture orientation is pinned by render tests);
* no dead-asset detection (unreferenced files, unused catalog entries);
* the `--preferred` warning does not fail a build;
* there is no CI, so every check above is run manually.

---

## 25. Uncertainties and known inconsistencies

The following are **not** settled contracts. Do not build new assets on them
without checking the implementation.

1. **Rotated fluorescent panels stretch their sheet.** A panel placed at 90°
   or 270° swaps its world X/Z extents while the sheet's UV axes stay put, so
   the 2:1 sheet is rotated and anisotropically stretched rather than rotated
   with the fixture. The source comment claims the sheet rotates with the
   fixture, and no test covers the rotated case. Treat 2:1 as the artwork
   contract and flag rotated fixtures for review.
2. **Glass-pane UV frame differs from wall art.** Glass panes use a local `u`
   anchored at the wall length origin and an upward `v`, unlike walls (world
   `u`, downward `v`). A glass texture does not phase-align with the wall it
   sits in. It is unclear whether this is deliberate.
3. **Decals are uploaded `REPEAT`.** Full-sheet decal UVs reach exactly 1.0,
   so bilinear edge sampling could bleed the opposite edge. The internal atlas
   insets its cells, but the external sheets do not. If a decal shows edge
   bleed, this is why.
4. **Emissive masks are unused by shipped content.** The feature is tested but
   has no production example; the mask's dimension independence is by code,
   not by shipped practice.
5. **Emissive-on-props has no authoring recipe.** The loader supports
   `emissiveTexture`, but the prop toolkit cannot emit one and no shipped
   model uses it. A prop mask must share the base map's UV frame.
6. **The icon is not copied by `tools/package.sh`.** `tests/test_package.py`
   asserts `icon.png`, but the packaging script does not include it in the
   payload. Whether the icon is consumed by an external step is unknown.
7. **Diagnostic sheets are not used by any level.** Their documented
   "asserted at the call site" claim refers to tests only.
8. **Interlaced PNGs are untested and rejected by the tiling tool.**
   `tools/textures/seam_repair.py` explicitly refuses interlaced input, so an
   interlaced tiling-sheet replacement fails the Python gate even if the Rust
   decoder handles Adam7 correctly.
9. **The rug atlas layout is pinned to the delivered 256×256 artwork.**
   `build_rug` uses explicit pixel regions (face rows 0–168, binding rows
   172–255) and requires the native 256×256 sheet; the other refreshed builders
   accept any legal 32/64/128/256 atlas. Resizing the rug atlas requires
   updating those regions, or the fitted UVs move.
10. **Prop textures and the repository texture policy.** Models embed PNGs
    for runtime loading and retain equivalent standalone PNGs under `assets/`.
    The audit checks source coverage independently of PNG compression. Larger
    masters are retained alongside native derivatives; editing a source alone
    does not update a GLB, which must be exported intentionally (§8.6).
11. **No minimum sizes and no per-class maximum below the global 1024, and the
    pack budget is aggregate.** A 16×16 surface sheet passes the dimension
    tests; the prop toolkit refuses to ship an embedded atlas above the native
    256×256 and the Rust props test enforces both that and the 64 MiB decoded
    pack budget, but a single pathological sheet below the global 1024 edge cap
    is otherwise unconstrained. Art direction is the only guard for the rest.
12. **Shipped fixture dimensions are pinned by a Rust test.** The loader test
    `test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog` asserts
    the exact dimensions of the three shipped fixture sheets and that every
    texel is opaque. Raising a shipped fixture's resolution requires updating
    that pin; the component itself is otherwise resolution-independent.
13. **`tools/props/build.py --check` has a narrower scope than the build
    path.** `--check` parses the GLB container only; UV range, budgets,
    scale and origin are enforced by the Rust tests and by the runtime parser.
15. **Fixture painters lag the shipped sheets.** `tools/textures/lights_art.py`
    still paints the panel at 256×128 while the shipped sheet is 1024×512; a
    plain `build.py` run skips it (dimension mismatch) and `--force` would
    downgrade the shipped artwork.
16. **Fixture painters and source images have different sizes.** The shipped
    fluorescent panel is 1024×512; its painter remains 256×128. Consult the
    catalog and this specification when replacing artwork.
17. **Pack material parsing is dormant.** `src/materials/pack.rs` still parses a
    pack `materials.json`, but the loader and compiler pass no pack, so no
    `pack:` id resolves from any level source.

---

## 26. Validation commands

Run from the repository root.

Asset tooling (Python 3, standard library only):

```sh
python3 tools/assets/validate.py                  # catalog + levels; exit 1 on error
python3 tools/textures/build.py --check           # PNG existence/dimensions; warnings don't fail
python3 tools/props/build.py --check              # prop GLBs exist and parse
python3 tools/assets/audit.py --workers 12        # full inventory + integrity/source checks
python3 tools/textures/seam_repair.py --check <png> [<png> ...]
python3 -m unittest tests.test_package            # full asset/environment/level gate
```

Rust:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Focused asset tests:

```sh
cargo test --workspace assets::tests
cargo test --workspace materials::tests
cargo test --workspace render::tests::test_shipped
cargo test --workspace props::tests::shipped_prop_assets
```

Regeneration paths (only when intentionally changing artwork):

```sh
python3 tools/textures/build.py [--only <id>] [--force --only <id>]   # surface/decal/fixture PNGs
python3 tools/props/build.py [--only <id>]                          # prop GLBs
python3 tools/props/animate_spooner_man.py [--check]                # the entity rig
```

Per §2, regeneration never replaces shipped artwork whose dimensions differ
from its painter's output unless `--force` is passed; a plain run is safe.
