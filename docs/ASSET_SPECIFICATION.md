# Places Asset Specification

This document is the canonical specification for every visual asset Places
draws from an image: world surface textures, decals, fixture faces, prop and
entity textures, normal maps, emissive masks and the non-runtime imagery that
ships with the repository (editor thumbnails, the app icon, documentation
screenshots).

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
   PNG under `assets/`, referenced through `assets/catalog.json` by logical id
   (or, for level packs, by path inside the pack). The one deliberate
   exception is a prop or entity texture: it is a PNG byte stream embedded in
   the model's `.glb` container (see §8). It is still committed artwork, but it
   is not a standalone `.png` file.

2. **No production texture imagery is generated from source code at runtime.**
   Do not paint textures in Rust, in shaders, in embedded pixel arrays or in
   draw commands, and do not synthesize a missing texture on demand when the
   application or a level loads. The shared untextured white sheet is a real
   committed PNG like every other texture (`core:tex_white_01`, §12.3). The
   few images the engine still generates (the HUD font atlas, the internal
   decal atlas, the missing-texture diagnostic, lightmap atlases and reflection
   probes) are internal machinery, listed in §12.4, and are not an authoring
   path. The level editor paints its own preview tiles in JavaScript; those are
   previews, not runtime assets, and are outside this policy.

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
   asset set for the Low quality profile — both profiles use the same files
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
  prop_proxies.json                  derived editor preview geometry (never hand-edited)
  README.md                          asset-system documentation
  levels/                            shipped level files (assets referenced by id)
  core/                              generic, theme-independent content
    decals/*.png                     shared decal sheets
    props/models/*.glb               shared props (embedded textures)
    textures/
      glass/*.png                    glass surface sheets
      floors/*.png                   generic floor sheets
      walls/*.png                    generic wall sheets
      normals/*.png                  tangent-space normal maps
      white_01.png                   shared untextured fallback sheet (§12.3)
  environment/
    <theme>/                         one directory per theme: office, pool, home
      props/models/*.glb             theme props (embedded textures)
      textures/
        walls/*.png                  wall surface sheets
        floors/*.png                 floor surface sheets
        ceilings/*.png               ceiling surface sheets
        lights/*.png                 light fixture faces
      decals/*.png                   theme decal sheets
  entities/
    <id>/model/<id>.glb              entities (embedded textures)
  diagnostic/
    textures/*.png                   engine test artwork; never used by shipped levels
```

Non-runtime imagery elsewhere:

```
level-editor/assets/thumbs/*.png     editor prop thumbnails (generated)
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
  alpha/response fields. A level may use any material on any surface.
* **Fixture faces live in `textures/lights/`.** They are catalog `light`
  entries, not `texture` entries; the fixture PNG is the `model` of the light.
* **Decal sheets live in `decals/`** — `assets/environment/<theme>/decals/`
  for theme art, `assets/core/decals/` for shared markings. They are catalog
  `decal` entries; the PNG is the `model`.
* **Prop and entity textures are embedded in the GLB** under
  `props/models/` (props) or `entities/<id>/model/` (entities). There is no
  `props/textures/` directory.
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
| Shared white sheet | `white_01.png` | **1:1** | 2×2 (`core:tex_white_01`) | none enforced | opaque white | no | no authored UV contract: it is a flat fill bound wherever a surface is untextured | engine fallback loaded once at startup; see §12.3 |
| Normal map | `normal_panel_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | alpha unused | **yes, both axes** | same UV as the albedo it augments | Surface quality class |
| Fluorescent panel face | `fluorescent_panel_01.png` | **2:1** | 1024×512 (current production) | none enforced | ignored (face is opaque) | no | full sheet; `u` across 1.2 m width, `v` across 0.6 m depth | POT both edges; replacement must stay 2:1 |
| Round downlight face | `pool_light_round_01.png` | **1:1** | 128×128 shipped; 256×256 or 512×512 to raise density | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius | POT both edges |
| Wall luminaire face | `pool_light_wall_01.png` | **2:1** | 128×64 shipped; 256×128 or 512×256 to raise density | none enforced | ignored | no | full sheet; `u` across 0.4 m width, `v` up 0.2 m height | POT both edges |
| Flush-mount diffuser face | `ceiling_light_round_01.png` | **1:1** | 256×256 shipped | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius (0.16 m) | POT both edges |
| Decal sheet | `no_diving_01.png` | **asset-defined**; placement must match it | 128×128 small markings; 1024×1024 hero signage | none enforced | **required cut-out**: alpha 0 background | no | full sheet fitted to the level placement's width × height | POT both edges |
| Prop / entity texture | embedded in `chair.glb` | **model-defined** (shipped 1:1) | 256×256 native (the normal shipped size); 32/64/128 legal for lighter props | none enforced | none: props always draw opaque | no | model `TEXCOORD_0`, normalized 0..1, clamped | hard 1024 engine limit; uniform resize safe, repack is not |
| Emissive mask | (none shipped) | **any**; must share the albedo's UV frame | ≤512 (Full budget) | none enforced | RGB sampled, alpha ignored | follows the albedo | same UV frame as the albedo | dimensions need not equal the albedo; a mask-only texture is exempt from the square-surface dimension test |
| Level-pack texture | `textures/*.png` in a `.zip` pack | 1:1 for tiling surfaces | any | none enforced | per pack material | yes for surface materials | same surface UV rules | no tooling validates pack contents |
| Diagnostic texture | `diagnostic_alt_01.png` | deliberately varied (96×64) | n/a | n/a | deliberately varied | n/a | not used by any shipped level | test artwork only |
| Editor thumbnail | `thumbs/desk.png` | 1:1 | 64×64 | 64×64 (generator) | RGBA, transparent background | no | n/a | generated by `tools/props/preview.py` |
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
| Soft warning threshold | above 256 px on either edge (`PREFERRED_TEXTURE_DIMENSION`); the shipped 1024² art is intentionally over it and Full uploads it unchanged |
| Hard maximum | 1024×1024 per edge (`MAX_TEXTURE_DIMENSION`, enforced by the decoder) |
| Power-of-two | not required for surfaces; a square NPOT sheet is policy-legal (`assets::tests`) and the 96×64 diagnostic proves non-square NPOT dimensions decode on the desktop GL path |
| Tileable | **yes, both axes**; seam-gated |
| Channels | RGB or RGBA; 8-bit output |
| Alpha | ignored unless the material authors `cutout` or `blend` |
| UV scale | world metres ÷ `tile_metres`, both axes |
| Wrapping | `REPEAT` + mipmaps |
| Filtering | the global filtering setting (linear by default; nearest optional), never per asset |
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
  is authored on a 25 cm motif cell and used at a 2 m repeat).

The stained variant is the same paper with damage; the damage must wrap too.
`wallpaper_stained_01.png` is one of the sheets whose seams were repaired with
`tools/textures/seam_repair.py` and its repair parameters are pinned in that
tool.

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

A fixture's **housing** (the office panel has none; the round and wall fixtures'
bezel, can and drum do) is ordinary body geometry drawn through the shared
untextured white sheet (`core:tex_white_01`, §12.3) with the profile's flat
authored shade. It is deliberately
untextured: the housing is metal/plastic body geometry, not artwork, and a
theme that wants patterned housing would introduce a fitted body sheet the way
the luminous face already is one. The texture-first contract therefore covers
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
| Geometry mapping | the face is a 1.2 m × 0.6 m rectangle (width along world X, depth along world Z) |
| UV layout | `u` spans the 1.2 m width (`u = 0` at min X, `u = 1` at max X); `v` spans the 0.6 m depth (`v = 0` at min Z / the −Z edge, `v = 1` at max Z) |
| Orientation | `v = 0` is the image's top row; the twin tubes run across the panel *width*, i.e. horizontally in the image |
| Current asset | 1024×512 (≈1.17 mm per texel both ways) |
| Preferred source resolution | 1024×512 |
| Higher resolutions | allowed while 2:1 and POT both hold: 128×64 → 256×128 → 512×256 → 1024×512 are the same layout. A resolution change to a *shipped* fixture sheet also requires updating the pin in `src/loader/tests.rs::test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog`, which asserts the exact shipped dimensions and full opacity. |
| Hard maximum | 1024 per edge, so 1024×512 is the largest valid 2:1 sheet |
| Transparency | none: no transparent padding, no cut-out, no alpha use |
| Emissive information | embedded in the artwork's brightness only; the glow is added by the renderer, and the base texture always multiplies the emission term |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

A `90°`/`270°` rotated panel placement is a **known code discrepancy**: the
renderer swaps the face's world X/Z extents but leaves the sheet's UV axes
fixed, so the 2:1 sheet is effectively rotated and stretched onto a 1:2 face
rather than rotated as a sheet (contrary to the comment in
`src/render/fixtures.rs`). No test covers rotated-panel UVs. Until that is
resolved, do not treat the rotated placement as an additional contract for the
artwork; keep the sheet 2:1 landscape and flag a rotated fixture for review
(§25.1).

The visible lens face is 0.4 × 0.2 m (`src/render/fixtures.rs`, pinned by
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
| Current asset | 128×128 |
| Preferred source resolution | 256×256 or 512×512 to raise density |
| Higher resolutions | allowed while 1:1 and POT hold, up to 512×512 (or 1024×1024) |
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
| Current asset | 128×64 |
| Preferred source resolution | 256×128 or 512×256 to raise density |
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
| Current asset | 256×256 |
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
| Fluorescent panel | `core:fluorescent_panel_01` | 2:1 | ceiling panel; `rotation_degrees` turns the panel |
| Round recessed | `core:pool_light_round` | 1:1 | ceiling downlight |
| Wall luminaire | `core:pool_light_wall` | 2:1 | needs `"mount": "wall"` and a world `"y"` |
| Flush mount | `home:ceiling_light_round` | 1:1 | residential ceiling lamp: a drum with a glowing diffuser disc |

Adding a further fixture family is a code change (a new `FixtureKind`, profile
and geometry in `src/lighting/tuning.rs` and `src/render/fixtures.rs`) plus an
asset. Do not add a fixture PNG without adding the family and its face
contract.

---

## 7. Decals and signs

Decals are local surface markings (floor arrows, hazard stripes, safety
signs). Two decal pipelines exist:

* an internal generated atlas (`core:decal_test_01` only), which is test
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

A new decal sheet does not need to be square, but it does need POT edges, an
alpha cut-out and a level placement whose width/height match its proportions.
If in doubt, author square artwork and place it square.

---

## 8. Model and prop textures

Prop and entity textures are different from world textures in almost every
respect. They are not tiles: they are fitted to a model's own UV map.

### 8.1 Format and location

* Models are self-contained binary glTF 2.0 files (`.glb`), read by
  `src/gltf.rs`. The supported subset is deliberately narrow: triangles only,
  `POSITION`, `TEXCOORD_0` and optional `COLOR_0`, 16/32-bit indices, no skins,
  no morph targets, no animation.
* **Textures are embedded PNG bufferViews inside the GLB.** External images
  and `data:` URIs are rejected with the message "external or data-URI images
  are not supported; embed the PNG in the GLB". A `.png` file next to a model
  is not used.
* The texture must be a PNG, and the parser rejects any embedded image above
  1024 px on either edge. A model with an invalid or oversized texture does
  not load partially: it falls back to its catalogue placeholder box.
* Shipped props carry one embedded sheet each. The runtime accepts up to 16
  images, 16 materials and 32 primitives per model, one material per primitive.

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
| Shipped sizes | 64×64 (8 models), 128×128 (16), 256×256 (9, including the refreshed domestic props and Spooner-Man) |
| Higher resolutions | The engine accepts embedded prop images up to 1024 px per edge (`MAX_PROP_TEXTURE_SIZE`) and downscales them to the active profile budget; no shipped atlas uses more than the native 256 because Full never samples a prop sheet above it |
| Hard maximum | 1024 px per edge (parser rejects the model above it) |
| Runtime budget | Full uploads prop sheets at ≤256 unchanged; Low at ≤128 (`TextureClass::Prop`); downscaling preserves aspect via one integer factor |
| Pack budget | 64 MiB decoded RGBA8 for the whole shipped pack (`PROP_TEXTURE_PACK_BUDGET_BYTES`); the current 33-prop pack is under 4 MiB |
| Decoded memory | `width × height × 4` bytes per image (RGBA8), summed over a model for `texture_bytes`; the surface class additionally caps one sheet at `MAX_SURFACE_TEXTURE_BYTES` (4 MiB) |
| Practical guidance | 256×256 is ordinary content, not a special high-quality variant; authoring above 256 only spends GLB bytes that no profile displays |

### 8.5 Materials, alpha and emissive maps

* The loader reads `pbrMetallicRoughness.baseColorTexture` and
  `baseColorFactor`, `emissiveFactor`, `emissiveTexture` and
  `KHR_materials_emissive_strength`. It ignores `metallicFactor`,
  `roughnessFactor`, `normalTexture`, `occlusionTexture`,
  `metallicRoughnessTexture`, `alphaMode`, `alphaCutoff`, `doubleSided` and
  samplers.
* **Props always draw opaque.** There is no per-prop cut-out or blend path;
  alpha in a model texture has no effect. The toolkit enforces this by writing
  full opacity into every prop sheet.
* `baseColorFactor` is baked into vertex colours; the renderer adds the
  material's emissive term on top, never multiplied by the baked light.
* An `emissiveTexture` is sampled with the *same* `TEXCOORD_0` as the base
  texture and only its RGB is used. It must therefore use the same UV frame as
  the base map. No shipped model declares emission; the path is exercised by
  tests.

### 8.6 Replacing a model texture

* **Uniform resize** (e.g. 128×128 → 256×256) keeps the layout and is safe.
  Keep the sheet square unless the model was authored otherwise.
* **Do not re-pack the atlas.** Editing pixels in place is safe; moving
  regions requires regenerating the GLB.
* **Do not exceed 1024 px** on either edge; the whole model fails if you do.
* The repository's regeneration path is the toolkit:
  `python3 tools/props/build.py --only <id>` (or
  `python3 tools/props/generate_spooner_man.py`). It validates UV bounds, scale,
  origin, triangle and texture budgets, and writes
  `assets/prop_proxies.json`. Refresh editor thumbnails with `--thumbs`
  afterwards.

### 8.7 Model geometry conventions (for context)

* 1 model unit = 1 metre; +Y up; +Z is the model's front at
  `rotation_degrees = 0`.
* The origin is the floor-contact point, horizontally centred under the
  model's bounding box (base at `y = 0`).
* The model's bounding box must match the catalog `size` within
  `max(2 cm, 6 % of the axis)`; the shipped-asset test and the prop tooling
  enforce it.
* Budgets: 500 triangles preferred, 800 needs justification, 1500 is the art
  budget; the engine loads up to 6000, then falls back to a placeholder.

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
    Full ≤512, Low ≤128.
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
* It is a **surface-class** texture for quality purposes: Full 1024, Low 256.
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

A `.zip` level pack may ship its own surface art next to its `level.json`:

* PNGs under `textures/` inside the pack;
* a `materials.json` mapping `pack:` material ids to those paths (or to
  catalog texture ids, which reuses shipped artwork).

Pack textures use the same decoder, so the format rules are enforced: PNG,
≤1024 px per edge, any accepted colour type. The layout rules are the
author's responsibility — tileable if the material repeats, square for tiling
surfaces — because the decoder cannot know how a sheet will be sampled. A pack
mapping that names a missing file is a named console error, not a silent
substitution.

A pack's `materials.json` supports the same fields as a catalog material
definition, including `emissive`, `emissive_intensity`, `emissive_mask`,
`normal_texture`, `normal_strength`, `specular`, `shine` (and the legacy
`roughness` inverse), `alpha_mode`, `opacity`, `alpha_cutoff` and the reflection
fields. Pack decals are a
limitation: a `pack:` decal id produces no geometry, because the decal pass
resolves only built-in patterns and catalog file decals.

There is no tooling that validates pack contents against this specification;
the pack author is responsible for the same contracts.

---

## 12. Non-runtime imagery

### 12.1 Editor prop thumbnails

* `level-editor/assets/thumbs/<short-name>.png`, one per placeable catalog
  entry, 64×64, RGBA with a transparent background.
* Generated by `tools/props/preview.py` and refreshed with
  `python3 tools/props/build.py --thumbs`.
* The editor test (`level-editor/tests/prop-assets.test.mjs`) fails if a
  thumbnail is missing; it checks existence and the PNG signature, not the
  64×64 size (that is a property of the generator). Staleness is not detected.

### 12.2 Application icon and documentation screenshots

* `icon.png`: PNG, square, RGBA, non-interlaced, ≤512 px; asserted by
  `tests/test_package.py`.
* `docs/screenshots/*.png`: 960×544 documentation captures. Not runtime
  assets; no contract beyond being still frames.

### 12.3 Shared untextured white sheet

`core:tex_white_01` → `assets/core/textures/white_01.png` is the renderer's
neutral fallback sheet: a solid opaque 2×2 white fill. Fixture housings, plain
body geometry and every texture slot with nothing better to bind sample it
(§6). It is deliberately tiny — it carries no artwork, no level references it,
and nothing derives detail from it — so the 2×2 size is a budget choice rather
than a UV or aspect contract; it must stay a fully opaque white fill.

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

### 12.4 Internal generated images (not authoring paths)

These images are produced by the engine and are deliberately not shipped as
PNGs. They are listed so no one mistakes them for assets to replace:

| Image | Producer | Purpose |
|---|---|---|
| 128×64 HUD font atlas | `src/font.rs` | project-owned bitmap UI font |
| 256×256 decal atlas (one live cell) | `src/render/decals.rs` | internal validation marking machinery |
| 64×64 missing-texture pattern | `src/materials/image.rs` | visible fallback for a broken texture |
| Lightmap atlas pages | `src/lighting/lightmap/` | baked light data, regenerated at level load; never a shipped asset. A developer path can dump a page as a PNG under `target/`, but that is a diagnostic capture, not an asset. |
| Reflection probe cubemaps | `src/render/reflections.rs` | baked per level load |

Diagnostic textures under `assets/diagnostic/textures/` are real PNGs but are
engine test artwork: the 96×64 sheet deliberately proves arbitrary NPOT
dimensions decode and the alpha sheet deliberately proves alpha decode. They
are excluded from shipped levels and from the dimension-contract test below.

---

## 13. Tiling requirements

**Tileable classes** (must join right→left and top→bottom):

* every environment surface sheet (walls, floors, ceilings);
* the core surface sheets used as surfaces: glass, linoleum, metal, plastic
  panels, grille and the normal maps;
* any level-pack surface texture used by a repeating material.

**Non-tileable classes** (never gated, never expected to wrap):

* fixture faces (fitted, `CLAMP_TO_EDGE`);
* decal sheets (fitted cut-out);
* prop and entity textures (model UVs, `CLAMP_TO_EDGE`);
* editor thumbnails, icon, screenshots and diagnostic sheets.

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

The source PNG is not necessarily what reaches the GPU. Two quality profiles
(`settings.json` → `"quality": "full" | "low"`, default `full`) set a runtime
edge budget per texture class:

| Texture class | Full (default) | Low |
|---|---|---|
| Surface sheet | 1024 | 256 |
| Fixture face | 1024 | 256 |
| Decal sheet | 1024 | 256 |
| Prop sheet (embedded) | 256 | 128 |
| Emissive mask | 512 | 128 |
| Lightmap atlas page | 1024 @ 12 texels/m | 512 @ 8 texels/m |

* Downscaling happens **once, at upload / level-load time**, through an
  integer-factor box filter that applies the same factor to both edges, so
  aspect ratio is preserved (up to per-edge rounding).
* The result is cached with the texture; nothing is rescaled per frame.
* **Full is the native presentation.** A 256×256 prop sheet and a 1024×1024
  surface upload unchanged; Full never resamples an asset that already sits
  within its class budget.
* Low is an optional quality/performance reduction, not a hardware
  requirement: it halves the native prop sheet (256 → 128) and quarters the
  sheets. It must never dictate the size of the asset stored in the
  repository.
* Both profiles use the same assets, ids, levels and geometry. Low is not a
  second art library. **Never author a separate low-resolution asset set.**
* No image class bypasses the budget. The font atlas, the decals' internal
  atlas and the lightmap pages are internal machinery, not shipped textures;
  the shared white sheet (`core:tex_white_01`, §12.3) is a catalog texture
  loaded once at startup at 2×2, far below every budget.

### 14.2 Why sources are kept large

The shipped environment surfaces are 1024×1024 on purpose. Full is the
default presentation and uploads them unchanged; keeping the source at the
hard limit means the same file still serves future renderer improvements and
any higher-resolution presentation without re-authoring. Low derives its 256 px
image from the same source. Replacing a 1024×1024 sheet with a 256×256 sheet
would visibly lower Full quality.

Prop sheets are the opposite case: the native size is 256×256, and Full
uploads it unchanged. The engine can still read a third-party GLB with images
up to 1024 px, but it downscales them to the profile budget, so the toolkit
refuses to *ship* embedded prop art above the native size rather than spend GLB
bytes on pixels no profile displays. Larger master artwork may be kept beside
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
  textures. POT edges are required for fitted sheets (fixture faces and decal
  sheets) because the ES 2.0 target cannot mip-map NPOT textures reliably.
  Surfaces may be square NPOT on the desktop GL path.

### 14.4 Preferred versus mandatory resolution (per class)

| Class | Mandatory | Preferred | Hard max |
|---|---|---|---|
| Environment surface | square | 1024×1024 (shipped); tooling warns above 256 | 1024 |
| Core surface | square | 1024×1024 (current); 128×128 painter output is contract-valid | 1024 |
| Normal map | square, tileable | 1024×1024 (current); 128×128 painter output is contract-valid | 1024 |
| Fluorescent panel | 2:1, POT | 1024×512 (current production) | 1024 |
| Round downlight | 1:1, POT | 128×128 shipped; 512×512 for density | 1024 |
| Wall luminaire | 2:1, POT | 128×64 shipped; 512×256 for density | 1024 |
| Decal sheet | POT both edges, cut-out alpha | 128×128 small; 1024×1024 hero | 1024 |
| Prop texture | model UV layout, no tiling | 256×256 native (the normal shipped size) | 1024 |
| Emissive mask | same UV frame as albedo | ≤512 | 1024 |
| Editor thumbnail | 64×64 | 64×64 | — |

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
or below the active profile's budget is displayed at its own size under that
profile, so there is no reason to author below the class's Full budget unless
the style wants it.

**Runtime-derived** — dimensions the engine reads rather than assumes.

* Decal quad proportions come from the level placement's `width`/`height`.
* Prop texture use comes from the model's UVs; the engine reads the decoded
  image's actual width/height.
* Emissive mask dimensions are never compared to anything.
* Level-pack textures are read from the pack at the size they are.

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
| Surface albedo, normal map, emissive mask | `REPEAT` | yes | global setting: linear (default) or nearest |
| External decal sheet | `REPEAT` | yes | global setting |
| Generated decal atlas | `REPEAT` | yes | global setting |
| Fixture face | `CLAMP_TO_EDGE` | yes | global setting |
| Prop/entity texture | `CLAMP_TO_EDGE` | yes | global setting |
| White fallback sheet (`core:tex_white_01`) | `CLAMP_TO_EDGE` | no | nearest |
| HUD font atlas | `CLAMP_TO_EDGE` | no | nearest |
| Lightmap atlas page | `CLAMP_TO_EDGE` | no | linear |
| Reflection probe cubemap | `CLAMP_TO_EDGE` | no | linear |

Consequences for artists:

* The user's filtering setting is global; you cannot make one asset nearest
  and another linear. Author detail that survives the default linear filter
  and the runtime budget. Fine 1-px detail in a 1024² sheet disappears at Low
  (256 px); keep important features at a scale that still reads.
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
| Shared white sheet: committed PNG, 2×2, opaque white, and what the renderer actually loads | Rust tests `assets::tests::the_shared_white_sheet_is_a_committed_opaque_white_png` and `render::tests::the_renderers_white_sheet_loads_from_the_committed_catalog_asset` | `cargo test` | yes |
| Fixture sheets: exact shipped dimensions and fully opaque | Rust test `loader::tests::test_fixture_sheets_resolve_one_sheet_per_family_from_the_catalog` | `cargo test` | yes |
| Fixture faces: POT, unique sheet, ≤1024 | `tests/test_package.py` | `python3 -m unittest tests.test_package` | yes |
| NO DIVING sign: 1024², RGBA, transparent pixel | `tests/test_package.py` | same | yes |
| Prop GLB: container parses, one mesh, `TEXCOORD_0` present | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) for a missing or unparseable model; budgets, UV range and scale/origin are not evaluated here |
| Prop GLB: UVs 0..1, triangle/vertex/texture budgets, scale/origin, PNG ≤1024 | Rust `props::tests`; runtime parser | `cargo test` | yes |
| Prop art budgets (triangles, native 256 px texture, 64 MiB decoded pack budget) | Rust `props::tests::shipped_prop_assets_match_the_catalogue_and_budgets` and its policy tests | `cargo test` | yes |
| Prop GLB decoded texture memory (per-texture and pack total) | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) |
| Editor thumbnails exist for every prop | Node test `level-editor/tests/prop-assets.test.mjs` | `cd level-editor && npm test` | yes |
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
* pool surface squareness and opacity (squareness is now covered by the new
  test; opacity is not asserted for pool sheets, though they are opaque in
  fact);
* decal non-POT is only a tooling warning for the small sheets (the new Rust
  test makes it a test failure for shipped decals);
* `tile_metres` is not compared against the painted repeat period;
* surface and decal orientation, and the "pale albedo" convention, are not
  machine-checked (fixture orientation is pinned by render tests);
* level-pack contents are not validated;
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
10. **Prop textures and the repository texture policy.** The repository rule
    says textures live as real PNG files under `assets/`; prop textures are
    committed as PNG byte streams inside their GLB instead. The asset
    documentation treats this as a deliberate exception (§8.1). A strict
    reading of the policy is not satisfied, but the design is intentional.
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
13. **The level editor paints its own preview textures.** The editor's 3D
    viewport generates wall/floor/ceiling preview tiles in JavaScript
    (`level-editor/js/viewport3d.js`). They are previews, not runtime assets,
    and are outside this specification; the game never loads them.
14. **`tools/props/build.py --check` has a narrower scope than the build
    path.** `--check` parses the GLB container only; UV range, budgets,
    scale and origin are enforced by the Rust tests and by the runtime parser.
15. **Fixture painters lag the shipped sheets.** `tools/textures/lights_art.py`
    still paints the panel at 256×128 while the shipped sheet is 1024×512; a
    plain `build.py` run skips it (dimension mismatch) and `--force` would
    downgrade the shipped artwork.
16. **`assets/README.md` and `docs/MAP_AUTHORING_GUIDE.md` lag the shipped
    fixture sizes** in their fixture tables (they state 256×128 for the panel).
    This specification and `tools/textures/README.md` carry the current sizes.
17. **Pack materials are richer than pack textures.** A pack's `materials.json`
    supports emissive, mask, normal, alpha and reflection fields exactly like a
    catalog definition; a `pack:` decal id, however, produces no geometry —
    pack decals are silently unsupported.

---

## 26. Validation commands

Run from the repository root.

Asset tooling (Python 3, standard library only):

```sh
python3 tools/assets/validate.py                  # catalog + levels; exit 1 on error
python3 tools/textures/build.py --check           # PNG existence/dimensions; warnings don't fail
python3 tools/props/build.py --check              # prop GLBs exist and parse
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

Editor assets (Node.js):

```sh
cd level-editor && npm test
```

Regeneration paths (only when intentionally changing artwork):

```sh
python3 tools/textures/build.py [--only <id>] [--force --only <id>]   # surface/decal/fixture PNGs
python3 tools/props/build.py [--only <id>] [--thumbs]                 # prop GLBs + editor thumbnails
python3 tools/props/generate_spooner_man.py                           # the entity
```

Per §2, regeneration never replaces shipped artwork whose dimensions differ
from its painter's output unless `--force` is passed; a plain run is safe.
