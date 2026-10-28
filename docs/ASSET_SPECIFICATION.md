# Places Asset Specification

Collection wayfinding plates (`collection:sign_*`) are file-backed RGBA PNG
decals under `assets/shared/decals/collection_*.png`. Their transparent perimeter,
upright typography and **4:1** aspect are mandatory: source 1024×256, placed at
matching width/height (for example 3.6×0.9 m). They fit once and never tile.
They use the existing decal cutout/depth-bias contract; no runtime text image
generation or new asset class is introduced.

Repository-wide checks: [authoritative desktop verification](VERIFICATION.md).

October 8, 2026 integration audit: colour/HDR, shared alpha sorting, prepared
lighting ownership and independent quality summaries were checked against the
current renderer/compiler. Asset aspect, UV, orientation and alpha contracts
remain unchanged; recorded stage images and platform campaigns retain their
original provenance.

October 10, 2026 Aero authoring: shared prop writer now accepts opt-in BLEND
and finite scalar opacity, preserving existing exports and model UV/PNG alpha
contracts. The 256-square Aero atlas retains its 1024-square master; surface
512-square derivatives retain 1024-square masters, fitted signage keeps its
1:2 or 1:1 aspect, and the sky uses the existing 2048×1024 panorama class.

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
   The renderer-owned caustic mask (§7.1b) is loaded by a fixed engine resource path,
   outside the map catalog; it is never a level-authored asset reference.
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
   (`src/engine/materials/image.rs::decode_png`). There is no JPEG, WebP, TGA, BMP,
   EXR or KTX path.

   Encoded inputs are bounded before allocation: PNG files at 32 MiB (including
   skies), self-contained GLB files at 64 MiB, `catalog.json` at 8 MiB and
   `gameplay.json` at 1 MiB. The regular-file readers check both metadata and
   the bytes actually read; growth during reading cannot bypass the bound.
   GLBs require an exact container length, one first JSON chunk, at most one
   BIN chunk, and one local buffer with matching declared length (allowing up
   to three alignment bytes). Existing decoded dimensions, mesh budgets, UV,
   alpha and orientation contracts still apply inside these encoded bounds.
   Material resolution checks the existing 1 GiB decoded level-image budget
   incrementally; fixture sheets share that budget and count shared image Arcs
   once. Optional catalog prefetch stages at most 64 images (256 MiB decoded).
   Completed or failed level loads trim optional CPU image retention to the
   existing 256-entry / 256 MiB limit; active worlds retain their own images.

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
   or `"blend"`. Decal sheets are always alpha cut-outs. Fixture faces are
   opaque. Prop/entity alpha follows glTF OPAQUE, MASK or
   BLEND on every supported mesh route (§8.5); OPAQUE ignores sheet alpha.

8. **Source artwork should retain as much quality as the hard limits allow.**
   The ordinary PNG hard ceiling is 1024 px on either edge; 2:1 sky panoramas
   have a separate 2048 px ceiling, enforced by the dedicated sky decoder.
   Store the best source within that limit; the engine decides how much of it reaches the GPU. Do not author a second, smaller
   asset set for a lower quality level — every level uses the same files
   (see §14).

9. **Tileable sheets must not introduce seams.** A tiling surface must join
   its own edges cleanly in both directions (§13). This is measured by
   `tools/textures/seam_repair.py --check` and by a Rust render test.

10. **Do not repack a model texture.** Model UVs are baked into the `.glb`.
    Uniformly resizing a model texture is safe because UVs are normalized
    fractions; moving or reordering texture regions invalidates the model's
    UVs and requires rebuilding the model (§8).

11. **The catalog is the registry for map-authored assets.** Levels reference logical ids, never file
    paths. The renderer-owned caustic mask uses its fixed resource path (§7.1b). A new surface material, decal or fixture is a catalog entry plus a
    PNG; the runtime resolves the file from the catalog.

---

## 3. Asset directory structure

```
assets/
  catalog.json                  authoritative logical registry
  README.md                     navigation and authoring commands
  shared/                       theme-independent assets (class `core`)
    props/*.glb, *.png           models and their standalone source sheets
    textures/<surface>/*.png    surfaces, masks, normals, effects and fallback sheets
    decals/*.png                shared fitted markings
    ui/*.png                    fixed-layout font atlas
  environments/<theme>/         Office, Pool, Home, Outdoor, Winter, Beach, Aero
    props/*.glb, *.png           themed models and standalone sheets/masters
    props/art/*.png              fitted painting artwork
    textures/<surface>/*.png    walls, floors, ceilings, lights, doors, water, sky
    decals/*.png                themed fitted markings
  entities/<id>/model/*.glb      skinned entities
  entities/<id>/textures/*.png   standalone entity sheets and masters
  diagnostic/textures/*.png     engine test artwork, never playable content
  references/<theme-or-entities>/  immutable concepts and authoring prompts
  levels/*.placesmap            ignored compiler outputs; never committed
levels/*.json                   maintained playable map sources
```

The logical IDs and `asset_class` values stay `core`, `environment`, `entity`
and `diagnostic`; physical directories do not restrict use. Any map may use
any catalog asset. The player resolves catalog paths relative to `assets/`.
Keep prop PNGs beside their GLBs; entity sheets stay in `textures/`. GLBs embed
runtime PNGs and the standalone equivalents remain committed. Do not create a
second prop texture directory or repack UVs while moving files.

Textures use the surface subdirectories shown above; fixture faces belong in
`textures/lights/`, normal maps in `shared/textures/normals/`, and decals in
`decals/`. Sky panoramas retain their 2:1 orientation and horizontal seam.
Runtime names retain `_01` variants; masters keep their existing suffixes.
Concept art and prompts are non-runtime references and must remain byte-identical.
A PNG is claimed by one catalog entry; models may embed shared identical sheets.

See [environment profiles](assets/ENVIRONMENTS.md) and
[entity profiles](assets/ENTITIES.md) for kit-specific placement, rig and
provenance details. Documentation captures live under `docs/images/` or
`docs/screenshots/`; `assets/shared/branding/places.png` remains the application icon.

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
| Floor surface sheet | `carpet_beige_01.png` | **1:1** | 1024×1024 (shipped) | none enforced | RGB shipped (carpet); ignored by default material | **yes, both axes** | world `(x, z)` ÷ `tile_metres` | office carpet is untinted; other floor materials may author a tint |
| Ceiling surface sheet | `ceiling_panel_01.png` | **1:1** | 1024×1024 (shipped) | none enforced | RGB shipped; ignored by default material | **yes, both axes** | world `(x, z)` ÷ `tile_metres` | ceiling material authors a grey tint |
| Core shared sheet (glass/floor/wall) | `glass_clear_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | per material: `blend` glass, `cutout` grille, opaque otherwise | **yes, both axes** | world metres ÷ `tile_metres` | seam-gated with the environment set |
| Steam effect sheet | `steam_01.png` | **1:1** | 256×256 (current production); 128×128 painter output is contract-valid | none enforced | **required**: a soft falloff reaching alpha 0 at every edge | sampled `REPEAT`, but its own edge alpha is 0 | one sheet per billboard; UV span = particle size ÷ the material's `tile_metres` | catalog `texture`; bound by the `core:steam_01`-style blended material; see §7.1 |
| Caustic intensity mask | `caustics_01.png` | **1:1** | 1024×1024 source | none enforced | opaque; RGB numeric intensity, alpha unused | **yes, both axes** | animated world `(x, z)` projection, freely rotatable | numeric linear mask modulates existing incident light; no albedo or emission; see §7.1b |
| Shared white sheet | `white_01.png` | **1:1** | 1024×1024 (current production); a flat fill, so any POT size is contract-valid | none enforced | opaque white | no | no authored UV contract: it is a flat fill bound wherever a surface is untextured | engine fallback loaded once at startup; see §12.2 |
| Normal map | `normal_panel_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | alpha unused | **yes, both axes** | same UV as the albedo it augments | Surface quality class |
| Fluorescent panel face | `fluorescent_panel_01.png` | **2:1** | 1024×512 (current production) | none enforced | ignored (face is opaque) | no | full sheet fit to the 1.12 × 0.56 m diffuser aperture; `u` across the width, `v` across the depth | POT both edges; replacement must stay 2:1 |
| Round downlight face | `pool_light_round_01.png` | **1:1** | 1024×1024 (current production); 128×128 painter output is contract-valid | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius | POT both edges |
| Wall luminaire face | `pool_light_wall_01.png` | **2:1** | 1024×512 (current production); 128×64 painter output is contract-valid | none enforced | ignored | no | full sheet; `u` across 0.4 m width, `v` up 0.2 m height | POT both edges |
| Flush-mount diffuser face | `ceiling_light_round_01.png` | **1:1** | 1024×1024 (current production); 256×256 painter output is contract-valid | none enforced | ignored | no | planar; sheet centre = fixture centre; inscribed circle = diffuser radius (0.16 m) | POT both edges |
| Decal sheet | `no_diving_01.png` | **asset-defined**; placement must match it | 128×128 small markings; 1024×1024 hero signage | none enforced | **required cut-out**: alpha 0 background. A catalog decal may add `"alpha_mode": "blend"` for a soft-edged feather sheet (path-to-grass strips); the default stays the hard 0.5 cut-out | no | full sheet fitted to the level placement's width × height | POT both edges |
| Prop / entity texture | embedded in `chair.glb` | **model-defined** (shipped 1:1) | 256×256 native (the normal shipped size); 32/64/128 legal for lighter props | none enforced | opaque by default; glTF `MASK` (+`alphaCutoff`, default 0.5) uses cutout and `BLEND` uses translucent passes on static, character and dynamic routes; prepared static transport follows §8.5 | no | model `TEXCOORD_0`, normalized 0..1, clamped | hard 1024 engine limit; uniform resize safe, repack is not |
| Sky sheet | `sky_aurora_01.png` / `sky_stars_01.png` | **2:1 equirectangular** | 2048×1024 preferred; stars 1024×512 | none enforced | RGB; alpha unused | **yes in u** (the horizon seam); v is a pole-to-pole span, clamped at the poles | sampled by view direction: `u` = yaw, `v` = pitch (`0` straight up). No level placement UVs | Hard maximum 2048×1024, POT both edges (`ShippedTextureKind::Sky`); the only non-square tiling-class sheet; catalog `texture` with `"surface": "sky"` |
| Emissive mask | (none shipped) | **any**; must share the albedo's UV frame | ≤512 (High budget) | none enforced | RGB sampled, alpha ignored | follows the albedo | same UV frame as the albedo | dimensions need not equal the albedo; a mask-only texture is exempt from the square-surface dimension test |
| Ultraviolet response mask | Arcade cosmic carpet | **same UV frame as its albedo** | 256×256 native; retained 1024×1024 master for Arcade | none enforced | numeric RGB; alpha ignored | follows the albedo | identical orientation, repeat and motif placement to albedo | black means no reaction, white full reaction; independent of emission |
| Blacklight batten face | `arcade:blacklight_tube` | **8:1** | 1024×128 master, 256×32 native | none enforced | opaque | no | full fitted sheet, horizontal u along the tube | 1.12×0.14 m lens inside a 1.2×0.15 m dark housing; face switches with the fixture |
| Diagnostic texture | `diagnostic_alt_01.png` | deliberately varied (96×64) | n/a | n/a | deliberately varied | n/a | not used by any shipped level | test artwork only |
| Application icon | `assets/shared/branding/places.png` | 1:1 | ≤512 | — | opaque RGB original; RGBA for new sources | no | n/a | non-interlaced; asserted by `tests/test_package.py` |
| Project/level thumbnail | `thumbnails/<hash>.png` in an authoring project | **16:9 landscape** | 1280×720 preferred; maximum 1280×720 | positive exact 16:9 | RGB8 or RGBA8, preserved | no | whole image, top-left origin; no crop/resize | at most 4 MiB encoded, 8 MiB decoder budget; PNG only, no APNG; stored once by hash in project packages; not a runtime material or custom asset importer |
| Documentation capture | `docs/screenshots/01-office.png` | 30:17 (960×544) | 960×544 | — | frame image | no | n/a | docs only |

**Do not read this table as "all textures are 1024×1024."** It is not a size
target; it is a per-class shape contract plus the resolution the current
production assets use. The hard ceiling is the only repository-wide number.

---

## 5. World surface textures

Surface textures are the tiling sheets used by wall, floor and ceiling
materials. They share one UV convention: `tiled_uv(a, b, tile_metres)` returns
`[a / tile_metres, b / tile_metres]` (`src/engine/render.rs::tiled_uv`), where `a`
and `b` are world-space metres along the surface, and the same period applies
to both axes. A material's `tile_metres` is authored in the catalog, validated
to `0.05..=64.0`, and defaults to `2.0`
(`DEFAULT_TILE_METRES`).

This means texel density is set by `tile_metres` and the source resolution
together. A 1024×1024 sheet on a 2 m repeat shows 2 mm per texel; the same
sheet at 256×256 shows 8 mm per texel. The *layout* is unchanged by
resolution.

The Arcade party-room wall uses `arcade:tex_wall_party_purple_01`: a square,
opaque, seamlessly tiling dark-purple painted stipple sheet. Its 1024-square
master is retained beside the 256-square runtime PNG, with a 0.8 m world repeat.
It has no emission or UV-reactive pigment; matching skirting uses the same
ordinary artwork and a modest satin response.

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

Floor contact behavior is separate material metadata. A catalog material
definition may author `ground_friction`, a finite coefficient in `0..=2` for
rigid-body contacts. Omitted coefficients resolve to **0.7** on ordinary ground
and **0.025** when `ground_surface: "ice"` is authored. The numeric coefficient
does not change player locomotion, PNG pixels, UVs, alpha, visual sheen or baked
lighting. The existing `ground_surface` normal/ice flag continues to select the
player's movement behavior. Contact friction multiplies the item's coefficient
by the contacted floor material's coefficient.

Representative catalog choices are `core:carpet_beige_01` and
`home:carpet_cream_01` at **1.0**, `home:hardwood_oak_01` and
`home:hardwood_walnut_02` at **0.6**, and `home:tile_home_01` plus
`core:pool_tile_deck_01` at **0.7**. These are explicit catalog values, not
runtime guesses from ids, colours or texture appearance. The shipped runtime
values live in the optional `assets/gameplay.json` sidecar, keyed by catalog id.
This strict version-1 document may override only `physics` on prop/entity entries
and `ground_friction` on material definitions. It cannot override images or any
visual material field. Unknown/duplicate ids, unknown fields and invalid physical
values reject the sidecar and catalog load.

Rigid profiles may use a bounded compound collider of 1..8 primitive or convex-hull parts;
offsets use local model-origin metres and uniform placement scale. Primitive
dimensions stay within `0.001..=100` metres and offset axes within ±100 metres
before and after scale. Convex hulls use 4..96 finite points, bounded local/scaled coordinates and a nondegenerate 3D volume. Fixed explicit shapes provide immutable rigid-item collision while static visual geometry and player standable boxes keep their existing contracts. Nested compounds are rejected. Optional `reset_group`
identifiers use ASCII `[A-Za-z0-9_-]`, 1..64 bytes, on movable bodies and connect
to the generic `reset_physics_group` action. These are physical descriptors;
the PNG, GLB geometry, UV layout, alpha and visual material contracts are unchanged.

Authored movable placements start dormant at the exact authored pose in every
map. This default is runtime behavior, without a catalog/sidecar field or map
opt-in. Pickup, explicit `activate_physics_group` and prescribed kinematic movement
activate them; meaningful active-body impacts activate dormant dynamic bodies.
Reset returns authored members to dormant. Solver sleep remains a separate state.
Runtime/developer spawning
starts active. Graphics rebuilds preserve item state, and this default requires
no image, GLB, map-format or prepared-render-package changes.

Render packages retain the exact raw `catalog.json` provenance that prepared
their geometry and lighting. Keep runtime-only physical tuning in the sidecar
so that changing it does not invalidate those packages or require a light bake.
Inline catalog physical fields remain supported for future catalog revisions;
the sidecar takes precedence only for the fields it explicitly supplies.

**Do not assume floors and walls are interchangeable.** They share the shape
and tiling contract, but a floor sheet is sampled on the ground plane and a
wall sheet on vertical faces; orientation and the material's response
(shine, specular, reflection) are authored per material, not per file.

### 5.3 Ceiling textures

Same rules again, with `(x, z)` UVs like floors. Ceiling materials in the
shipped catalog author a dimming tint (for example `[0.72, 0.72, 0.70]` for
the office panel), so the source artwork is painted pale/neutral (see §18).

### 5.4 Carpet

Carpet is not a separate material class; it follows the floor surface sheet
contract in §5.2, including the material's authored tint:

* the office carpet material is untinted, so its source artwork carries the
  completed warm-brown albedo;
* Arcade carpet materials use the bounded linear tint `[0.4, 0.4, 0.4]` to keep
  the navy pile and fluorescent pigments restrained. Their ultraviolet response
  uses the same tint with a separate numeric pigment mask (see §9.1);
* it must tile seamlessly;
* it must be square.

The render test `test_carpet_png_has_no_metre_checker` additionally asserts
that the shipped carpet has no artificial "metre checker": quadrant means
differ by no more than 4 levels and the pile varies by at least 4 levels.
That test is specific to the shipped office carpet; for new carpet artwork the
practical rule is the same: the texture must not expose its repeat grid as an
obvious pattern.

### 5.5 Wallpaper

Wallpaper is a wall surface sheet. Requirements:

* square, 1024×1024 production size;
* tileable in both directions — the right edge must join the left and the top
  the bottom with no visible wrapped step;
* pale/near-neutral albedo where the material tints it
  (`core:wallpaper_yellow_01` uses tint `[0.85, 0.80, 0.42]`);
* the repeat must read at the material's `tile_metres` (the office wallpaper
  uses restrained vertical pinstripes with staggered elongated lozenges across
  its 2 m repeat).

The stained variant is the same paper with damage; the damage must wrap too.
The vertical print preserves the pale cream/beige palette and material tint.
Both sheets are loaded from PNG by `office_art.py`, including forced builds.
The stained sheet was seam-repaired with `--band 32 --residual-band 12
--radius 12 --offset 16`; the clean sheet passed without seam repair.

### 5.6 Tile, concrete, metal, plastic, glass and grille

* **Tile** (pool deck/basin/wall) — square 1024×1024, tileable, opaque. The
  painted grid must be regular at the material's `tile_metres` (deck 1.5 m,
  basin 1.0 m, wall 2.0 m). A tile sheet is still a *surface* sheet: the same
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
* **Concrete and tiling artwork** — the catalog's `core:painting_dull_01`
  material reuses the wallpaper texture. New tiling sheets follow the ordinary
  surface contract (1:1, tileable). Fitted hanging paintings use §7.2 instead.

---

## 6. Light fixture faces

The completed Office `core:fluorescent_panel_01` and Pool
`core:pool_light_round` / `core:pool_light_wall` fixtures now draw their accepted
GLB models through ordinary model materials. Their original authoring IDs,
mounts, circuits and emission controls remain. Only luminous material slot 2
receives source tint, emission strength and animation; metal and gaskets retain
ordinary receiver lighting. The PNG fixture sheets below remain the procedural
fallback contract and continue to serve the other fixture families.

The Office export retains its 1.2 × 0.6 × 0.045 m housing, downward
1.12 × 0.56 m aperture and twin covers reaching 40 mm below the mount. Pool's
recessed disc has radius 0.214 m at mount Y−0.017. Its finite source samples stay
inside that disc. Pool's wall model has two separate 0.354 × 0.048 m luminous
strips centered at local Y±0.045, with their fronts at local Z=0.110. Two
half-power sources sit 1 mm in front of those strips; the central gasket emits
nothing. The Arcade panel keeps its existing procedural diffuser plane.

A fixture's mesh is generated from a fixture profile
(`src/engine/lighting/tuning.rs::fixture_profile`); its *visible luminous face* is a
catalog `light` entry whose `model` is a PNG. Fixture sheets are **fitted**:
the whole sheet is mapped once across the face, the UVs never leave `[0, 1]`,
and the renderer uploads them `CLAMP_TO_EDGE` with mipmaps. Nothing about a
fixture face tiles.

The luminous face uses the fitted sheet's artwork multiplied by the placed
light's source `color` and visible `emission` strength (defaulting to
`brightness`). White diffusers therefore show the authored light colour;
coloured lens artwork retains its detail under that tint. Source switching
and emission animation scale this same coloured term. The sheet is not a
lightmap and does not cast illumination; housing remains separate geometry.
There is no separate emissive map for a fixture face.

A fluorescent panel's **housing** uses the non-emissive
`core:fluorescent_panel_housing` material. It reuses the committed
`shared/textures/walls/metal_panel_01.png` albedo and the ordinary brushed-metal
normal and sheen response, with `reflection_mode: "none"`; a plain fluorescent
fixture therefore adds no probe-capture requirement. UVs follow the material's
physical repeat on each body face rather than stretching a whole sheet across
a border strip.
Static uses baked vertex receiver illumination; Full uses the ordinary
prepared lightmap contribution path, including circuit on/off changes.
Housing emits no light and does not use the diffuser's source tint or emission
animation. It can receive coloured illumination like any ordinary surface.

Other fixtures' bezel, can and drum retain their authored untextured body
shades through the shared white sheet (`core:tex_white_01`, §12.2). The fitted
face contract covers the diffuser, lens or panel artwork. The round Home
flush mount in particular draws its whole visible face, rim line and centre
structure from
`environments/home/textures/lights/ceiling_light_round_01.png`.

Procedural bodies receive ordinary baked lighting and cast opaque Full transport
shadows. Their mounting backs are closed, including the round and wall model
fallbacks; luminous fitted sheets alone are omitted from analytic-source
occlusion. The blacklight source uses its 1.12 × 0.14 m lens at mount Y−0.046.
Home's source sits on its diffuser at mount Y−0.070, with a square inscribed in
the 0.148 m aperture. These source positions and the visible geometry share
constants. Static retains its coarse visibility model, without fixture
self-occlusion. No body emits or inherits the placed source tint.

Fixture faces are **opaque by construction**: the face draws in the opaque
pass and its alpha channel is ignored. A shipped fixture sheet is additionally
required to be fully opaque by this asset contract,
so RGB artwork and an RGBA file whose alpha is everywhere 255 are equivalent
in practice.

### 6.1 Fluorescent ceiling panel

Shipped asset: `core:fluorescent_panel_01` →
`assets/environments/office/textures/lights/fluorescent_panel_01.png`.

| Property | Value |
|---|---|
| Aspect ratio | **2:1, landscape (mandatory)** |
| Geometry mapping | the sheet is fitted to the diffuser aperture: a 1.12 m × 0.56 m rectangle inset inside the fixture's 1.2 × 0.6 m troffer footprint (width along world X, depth along world Z), recessed 0.012 m above the frame's bottom |
| UV layout | `u` spans the 1.12 m aperture width (`u = 0` at min X, `u = 1` at max X); `v` spans the 0.56 m depth (`v = 0` at min Z / the −Z edge, `v = 1` at max Z) |
| Orientation | `v = 0` is the image's top row; the twin tubes run across the panel *width*, i.e. horizontally in the image |
| Current asset | 1024×512 (≈1.09 mm per texel both ways) |
| Preferred source resolution | 1024×512 |
| Higher resolutions | allowed while 2:1 and POT both hold: 128×64 → 256×128 → 512×256 → 1024×512 are the same layout. Preserve each family's mandatory aspect, POT edges, hard limit and full opacity during resolution changes. |
| Hard maximum | 1024 per edge, so 1024×512 is the largest valid 2:1 sheet |
| Transparency | none: no transparent padding, no cut-out, no alpha use |
| Emissive information | embedded in the artwork's brightness only; the glow is added by the renderer, and the base texture always multiplies the emission term |
| Filtering / wrapping | mipmaps; `CLAMP_TO_EDGE` |

The 1.2 × 0.6 m outer footprint is the *housing*: four brushed-metal side walls
drop 0.045 m from the ceiling plane, a bottom frame borders the diffuser on all
four sides, a full opaque backplate closes the mounting footprint and the diffuser
sits 0.012 m recessed above the frame's bottom. Only the fitted sheet glows; the housing binds
`core:fluorescent_panel_housing` with zero emission and ordinary receiver lighting.

A `90°`/`270°` placement uses the fixture's existing quarter-turn convention:
local +X becomes world +Z and local +Z becomes world −X. The frame borders and
fitted UVs turn together, so the aperture is 0.56 × 1.12 m in world X/Z while
the same landscape PNG keeps its local 2:1 layout. `u` runs along +Z and `v`
along −X; the artwork is rotated without mirroring. Both orientations show
1.09 mm per texel in each direction at 1024×512. The finite light source uses
that same luminous aperture and diffuser plane; the metal body remains opaque.

The visible lens face is 0.4 × 0.2 m (`src/engine/render/common/fixtures.rs`, pinned by
test). The map guide's separate "0.4 × 0.18 m" figure for the wall luminaire
describes the *bake's light rectangle* (`half_width: 0.20`,
`half_depth: 0.09` in `src/engine/lighting/tuning.rs::fixture_profile_for_kind`), not
the lens artwork; both figures are current and describe different things.

### 6.2 Round recessed downlight (pool ceiling light)

Shipped asset: `core:pool_light_round` →
`assets/environments/pool/textures/lights/pool_light_round_01.png`.

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
`assets/environments/pool/textures/lights/pool_light_wall_01.png`.

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
`assets/environments/home/textures/lights/ceiling_light_round_01.png`.

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

The ten-sided drum has an opaque mounting back. Its 0.16 m body radius and
0.012 m rim leave a 0.148 m luminous outer radius; the fitted UV frame still
uses the original 0.16 m radius, preserving the existing artwork exactly.

### 6.5 Ceiling lights and wall lights, summarised

There is no separate "ceiling light" or "wall light" texture class. A placed
light names a fixture id; the fixture id selects one of the three families and
therefore one of the three face contracts:

| Family | Example id | Face aspect | Placement notes |
|---|---|---|---|
| Fluorescent panel | `core:fluorescent_panel_01` | 2:1 | ceiling troffer: the sheet is fitted to the inset diffuser inside a non-emissive brushed-metal frame; `rotation_degrees` turns the panel |
| Round recessed | `core:pool_light_round` | 1:1 | ceiling downlight |
| Wall luminaire | `core:pool_light_wall` | 2:1 | needs `"mount": "wall"` and a world `"y"` |
| Flush mount | `home:ceiling_light_round` | 1:1 | residential ceiling lamp: a drum with a glowing diffuser disc |

Adding a further fixture family is a code change (a new `FixtureKind`, profile
and geometry in `src/engine/lighting/tuning.rs` and `src/engine/render/common/fixtures.rs`) plus an
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

The commercial replacement is **`core:ceiling_diffuser_01`**, a static GLB prop
with a 0.6 × 0.065 × 0.6 m outer frame, closed back and four-way concentric inset
louvers. Its base-centred origin remains at minimum Y=0; mount it at ceiling Y
minus 0.065 m. It has no collision or emission. Full uses ordinary model receiver
charts and circuit layers; Static uses the existing baked on/off prop variants.
Its square opaque painted-metal sheet is
`assets/shared/props/ceiling_diffuser_01.png` (256×256 native), with the retained
1024-square ImageGen master alongside it. Every geometric face fits the full
sheet in [0,1] without tiling. Recess depth, gaps and sloping louver faces are
geometry, so the bitmap contains only paint grain, no baked illumination.
The old decal remains readable for historical/test sources; active Arcade vents
use the model at their original ceiling centres.

A new decal sheet does not need to be square, but it does need POT edges, an
alpha cut-out and a level placement whose width/height match its proportions.
If in doubt, author square artwork and place it square.

### 7.1 Steam effect sheets

An **effect sheet** is the artwork of a level's `effects[]` billboards: the
ambient steam plumes (`src/engine/render/common/effects.rs`,
`src/engine/render/wgpu/effects.rs`). It is neither a world surface nor a decal: the
level authors an emitter volume, the renderer generates camera-facing quads,
and the sheet is sampled once per particle.

| Property | Value |
|---|---|
| Aspect ratio | **1:1 (square)** |
| Preferred source resolution | 256×256 (the shipped `steam_01.png`); 128×128 painter output is contract-valid |
| Hard maximum | 1024 per edge, like every ordinary PNG |
| Channels | RGBA is effectively required: the alpha channel is the puff |
| Alpha | **required**, and must fall to 0 at every edge. The particle's own fade-in/out envelope multiplies it, and the material's `opacity` multiplies again |
| Tileable | sampled `REPEAT`, and the zero edge alpha is what keeps a repeat invisible; the artwork itself is a radial puff, not a seam-matching pattern |
| UV contract | one billboard per particle; `u`/`v` run `0..size / tile_metres`, where `size` is the effect's authored particle size and `tile_metres` the material's period. The shipped material uses `tile_metres: 0.5` |
| Orientation | a puff is rotationally symmetric, so orientation is not part of the contract; keep the artwork centred and understated |
| Catalog shape | a `core` `texture` entry (`core:tex_steam_01` → `shared/textures/effects/steam_01.png`) plus a `source: "definition"` `material` entry (`core:steam_01`) with `alpha_mode: "blend"`, a low `opacity`, no tint, no emission and no reflection |
| Painting path | `tools/textures/extra_art.py::build_steam`, regenerated by `python3 tools/textures/build.py --only core:tex_steam_01`; deterministic helpers only |

An effect billboard is never collidable, never occludes baked light and is not
part of the lighting bake. The RGB channels are sampled as authored and
multiplied by the particle's alpha; keep the puff white-grey and place all of
the shape in the alpha channel.

### 7.1a Snow weather sheet

`core:tex_snowflake_01` is a committed 256×256 square RGBA PNG at
`assets/shared/textures/effects/snowflake_01.png`. Preferred source size is
256×256; ordinary hard limit is 1024 per edge. Full UV `0..1` fits one
soft irregular granule to each camera-facing weather quad, independent of
flake size and material tiling. All four edges must have zero alpha.
The white centre and subdued blue-grey rim retain contrast on dark sky and
pale ground. No directional orientation, emission, reflection or tiling is
required. The shared effect sampler uses repeat; transparent margins avoid
seams. Straight alpha is multiplied by weather fades and material opacity.
The same full-UV square sheet also supports velocity-aligned elongated
billboards in storm weather; only the quad geometry stretches, while PNG
aspect, UV orientation, straight alpha and transparent borders remain intact.
The corresponding blend material is `core:snowflake_01`. The PNG is artwork
from the built-in imagegen tool, downsampled with alpha preserved; the game
never synthesizes it. See `src/engine/render/common/snow.rs` for geometry and fades.

### 7.1b Caustic intensity mask

The renderer loads the committed
`assets/shared/textures/effects/caustics_01.png` by its fixed engine resource path. This is a numeric light-focus
mask, rather than a billboard, surface albedo, normal map or emissive mask.
It contains only slender, softly feathered irregular white/grey ridges over
broad dark cell interiors; no tile grout, surface colour, directional shading,
pool scene or baked illumination belongs in the sheet. The renderer samples
the same sheet twice with counter-moving world-XZ coordinates to vary the
bands smoothly. Existing source illumination supplies their colour and energy;
the mask must not emit light or brighten an unlit pool independently.

The mandatory contract is a square 8-bit PNG, seamless on **both** axes,
with equal RGB channels and implicit or explicit opaque alpha (255).
Preferred source size is **1024×1024**; the ordinary **1024 px per-edge** hard
limit applies, with no enforced minimum. Black is zero intensity, white is
maximum, and grey is a numeric intermediate. Upload as `DataLinear`
(`Rgba8Unorm`), never sRGB. Repeat wrapping, numeric-average mipmaps and the
fixed trilinear 4× sampler (1× on adapters without anisotropy) apply, like a lighting-data resource. World X/Z projection may rotate
or offset the sheet freely; there is no north, fitted UV frame or dependency
on the receiver's albedo repeat. Texture quality controls its upload budget
independently of Static/Full lighting: `TextureClass::EmissionMask` fits High to 512, Medium to 192 and Low to 128 pixels. The mask owns no material tint,
normal response, reflection, transparency or emission.

This is renderer-owned lighting artwork with no map catalog entry or surface material.
Its fixed path keeps map dependency identities independent of the effect; the
normal asset export includes the shared texture directory. A missing or invalid
PNG disables caustics with a diagnostic. The bounded Rust PNG-contract test
checks the source dimensions, numeric channels and opacity.
Keep replacements under `shared/textures/effects/` and preserve `_01` naming,
opaque numeric channels, square aspect and both seamless wraps. Validate with
`python3 tools/textures/seam_repair.py --check
assets/shared/textures/effects/caustics_01.png`, the PNG-contract test, and a repeated-sheet visual inspection. This numeric contract is
separate from steam/snow because its RGB controls incident-light modulation,
while those sheets use alpha-covered particle artwork.

Provenance: generated with the built-in `image_gen.imagegen` tool on October 15,
2026, `transparent_background: false`, without input images. The original
opaque RGB output was 1254×1254, SHA-256
`d233a316ffd763b4c60c032ddbea0221f4e09c07c546fba7377c0fc149b065f4`.
The saved production derivative converts it to grayscale RGB and resizes once
to 1024×1024 with Pillow Lanczos; no texture art is synthesized by that
conversion or at runtime. The original generated artwork already passed the
repository seam metric; no seam repair was applied.

Production PNG: 533,201 bytes, SHA-256
`2dd38a6e4b496ade2d718b4a4ef5ddc8667b5094e5d6e3231cbdeae114f222d4`.
The converted PNG passes both raw and smoothed wrap gates; its repeated 2×2
sheet was visually inspected. Exact generation prompt:

```text
Use case: stylized-concept
Asset type: production numeric caustic intensity mask for a low-poly swimming-pool renderer; one 1024 by 1024 square opaque PNG.
Primary request: create a perfectly seamless repeating monochrome underwater caustics intensity texture. Narrow soft white and grey irregular water-focus bands form connected distorted organic cells over a mostly near-black field. Cell sizes and band brightness vary naturally, with several gently angular bends that suit restrained low-poly Places art. Leave broad dark interiors between bands. Focus ridges are smooth antialiased, slender and softly feathered, never hard outlined polygons.
Composition: flat top-down texture only, filling the entire square uniformly, exactly tileable on both X and Y. Opposite edges continue every crossing band at matching position, angle and brightness. Use a varied network across roughly six to nine cells per edge; avoid an obvious centre or directional gradient.
Color palette: strictly neutral grayscale numeric intensity: black is zero, white is maximum, mid-grey is partial intensity; all RGB channels equal; opaque alpha 255 throughout. No colour tint, transparency or premultiplied alpha.
Constraints: no pool tile or grout, no albedo, no physical surface, no perspective, no directional illumination, no baked shadows or shading, no vignette, no frame, no text, no watermark, no scene objects, no diagonal streak motif. This is only a numeric mask to multiply existing source lighting; do not depict a lit swimming pool. Keep the mask restrained and mostly black, with natural nonuniform thin caustic ridges. Seamless repeating artwork is mandatory.
```

### 7.2 Framed hanging paintings

Paintings pair a file-backed decal with a separate, shallow GLB frame. The
artwork uses the decal texture budget (High ≤1024), avoiding the prop atlas's
256-pixel runtime cap. The frame follows the ordinary prop geometry and
embedded-texture contract (§8); its front is +Z and its base is y=0.

`core:decal_temptation_adam_eve_01` ("The Temptation of Adam and Eve") keeps
the supplied photograph's decoded **900×546 pixels**, upright and unmirrored,
without cropping, resampling or colour adjustments. The PNG is a **1024×1024
RGBA** sheet: the photograph occupies `[62, 239, 962, 785)` and the surrounding
padding is transparent. The fitted square quad must therefore be **2.048 m ×
2.048 m** at scale 1, giving the visible painting **1.800 m × 1.092 m** and
preserving the original 150:91 aspect ratio. High uploads the whole sheet
unchanged; Medium and Low apply the ordinary decal budgets.

`core:painting_frame_landscape_01` is a **1.900 m × 1.192 m × 0.060 m** timber
open-backed frame with a 50 mm border. It reuses the committed
`assets/shared/props/bookshelf.png` atlas. The painting's centre in model
space is `(0, 0.596, -0.030)`, on the wall at the frame's rear plane; the decal renderer
adds its usual depth bias. Rotate this offset with the frame, and set the
decal's `surface` to match its facing direction. Prop `y` is relative to the
local floor; decal `y` is absolute world height. When scaling the pair, scale
the quad, centre offset and frame together. The frame alone carries no artwork.

The demo mounts the pair on the Home's west wall, facing east. Its frame
instance carries the title label through the existing E interaction. Keeping
the image on the real wall preserves the standard decal parent/depth contract;
the frame rails provide the painting's physical depth.


---

## 8. Model and prop textures

Prop and entity textures are different from world textures in almost every
respect. They are not tiles: they are fitted to a model's own UV map.

### 8.1 Format and location

* Models are self-contained binary glTF 2.0 files (`.glb`), read by
  `src/engine/gltf.rs`. The supported subset is deliberately narrow: triangles only,
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
source / master artwork        (optional, e.g. 512x 512 or 1024x1024,
        |                       kept beside the model as an authoring source)
        v
asset build pipeline           (tools/props: embeds the native runtime sheet;
        |                       refuses to ship above the native size)
        v
normal runtime texture         256x 256 native
```

| Property | Value |
|---|---|
| Aspect ratio | model-defined; shipped models use square sheets, but the loader accepts any shape up to 1024² |
| Native size | **256×256** is the normal shipped prop texture size (`PROP_TEXTURE_NATIVE_SIZE`); 32/64/128 remain legal for lighter props |
| Shipped sizes | 363 embedded PNGs across 335 models: 32-square (23), 64-square (6), 128-square (74), 256-square (246), 512-square (6), 1024-square (8); `tools/assets/audit.py` supplies the current inventory |
| Higher resolutions | Reviewed party cloths use 1024-square sheets and wrapping uses 512-square sheets for print fidelity at their model scale; ordinary props retain 256 or smaller |
| Hard maximum | 1024 px per edge (parser rejects the model above it) |
| Runtime budget | High preserves sheets up to 1024; Medium ≤192 and Low ≤128 (`TextureClass::Prop`); smaller sheets remain unchanged and downscaling preserves aspect |
| Pack budget | 128 MiB decoded RGBA8 for the entire shipped pack (`PROP_TEXTURE_PACK_BUDGET_BYTES`), counting unique family images and numeric masks; the phase 6 audit reports 89.434 MiB by exact PNG identity |
| Decoded memory | `width × height × 4` bytes per image (RGBA8), summed over a model for `texture_bytes`; the surface class additionally caps one sheet at `MAX_SURFACE_TEXTURE_BYTES` (4 MiB) |
| Practical guidance | 256×256 remains ordinary content; larger sheets require reviewed metre-scale print detail and bounded whole-pack accounting |

### 8.5 Materials, alpha and emissive maps

* The loader reads `pbrMetallicRoughness.baseColorTexture` and
  `baseColorFactor`, `emissiveFactor`, `emissiveTexture`,
  `KHR_materials_emissive_strength`, `alphaMode`, `alphaCutoff`, and scalar
  `metallicFactor`/`roughnessFactor`. Scalar response is the existing compact
  sheen model, not a full glTF PBR implementation. Omitted controls preserve
  the matte legacy response (roughness 1, metal 0). It ignores `normalTexture`,
  `occlusionTexture`, `metallicRoughnessTexture`, `doubleSided` and samplers.
* **Props default opaque; `alphaMode: "MASK"` selects cutout.** A
  masked primitive becomes the engine's alpha-tested cutout pass at the
  authored `alphaCutoff` (glTF default 0.5); this is what grass tufts and tree
  leaf cards use. The toolkit writes full opacity into every prop sheet unless
  the builder opts in with `p.set_texture(size, alpha=True)` and
  `p.material(..., alpha_mode="mask")`.
* **`alphaMode: "BLEND"` is the blended-material contract.** The importer reads
  it as a real translucent material. Static props, skinned characters and
  dynamic objects draw it through their translucent passes. World/prop/dynamic/
  character draws share stable back-to-front centre sorting in scene and emission.
  Intersecting triangles and several primitives within one object still need
  careful placement. MASK uses the cutout pass on every route.
  A glTF `baseColorFactor` alpha is folded into the vertex
  colours by the importer, so the blend contract itself carries opacity `1.0`;
  per-instance fade is a runtime component, not an asset property.
  The shared prop writer authors this through
  `p.material(..., alpha_mode="blend", opacity=0.28)`. Scalar opacity must be
  finite in 0..1 and belongs only to BLEND; the writer preserves existing
  opaque/MASK bytes and leaves PNG alpha unchanged. RGB tint remains optional.
* An emissive material is one slot of a model and lights nothing by itself
  (`emissiveFactor`/`KHR_materials_emissive_strength`, optional
  `emissiveTexture` mask). The Halloween entities use this for the carved
  candle flame, the sheet ghost's cloth (with its face-feature primitive kept
  much dimmer so the painted face stays readable) and the pumpkin head; their
  actual illumination is the runtime `glow` component, never a baked static
  light.
* Prepared static transport (Medium/Full lightmaps) tests real triangle-hit
  coverage: numeric level-zero bilinear texture alpha × vertex alpha × material
  opacity. OPAQUE always blocks, MASK blocks at or above its cutoff, and BLEND
  transmits `1 - coverage` along a straight ray. Separate translucent layers
  multiply transmission; a shared triangle edge counts once. BLEND does not
  refract, tint transmitted light, or supply diffuse bounce. These semantics
  apply to static architectural sheets and static model primitives alike;
  model UVs clamp and architectural UVs repeat. Water retains its separate
  volume attenuation. A genuinely open aperture contains no surface to attenuate.
* The historical vertex-lighting/fast-occluder path still derives coarse boxes
  without texture alpha. Foliage placements use `"occludes": false` for that
  path; this flag does not remove drawn opaque or covered MASK triangles from
  the prepared static solve. Collision remains independent of visual alpha.
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
* `home:floor_lamp` uses ordinary `lamp_body` on its base/stem and the unchanged
  emissive `linen_shade` only on the shade. `home:cabinet_strip` uses ordinary
  `strip_housing` and emissive `warm_diffuser`. Their source builder registers
  the body material before emitting any geometry; registering the first
  luminous material afterwards would otherwise reassign existing slot-zero
  body triangles. Fitted PNGs and luminous material values are unchanged. The
  strip housing underside has a real aperture at the unchanged lens perimeter;
  four trapezoids preserve the original affine UVs around it, eliminating the
  former exposed coplanar housing/lens faces without a depth offset.
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
  The October14 complete rebuild supersedes its earlier body-only refinement.
  `completed-models.json` guards the adopted export; final editable masters remain
  in Downloads. The superseded body-only refiner and its duplicate checker were
  retired; the original source GLB and accepted export remain preserved. The earlier
  [refinement report](reports/release-0.8.0.md#spoonerman-body-refinement) remains dated evidence;
  original head/PNG/rest-transform preservation does not describe the new export.

The refined sign/fixture/CRT builders also load standalone PNG source atlases
beside their GLBs. Preserve their existing region frames: stop sign uses the
2×2 sign/pole/bracket/back layout; the rebuilt exit sign uses face
`(0,0,256,128)`, body `(0,128,128,128)` and metal `(128,128,128,128)` on a
256² sheet. Its 2:1 face is recessed 6 mm into a closed bevelled housing;
only that face uses the emissive material. Globe and switch retain their three-cell 128² layouts; CRT retains
screen/bezel/body/panel on a 128² 2×2 sheet. The mannequin uses one opaque
1024² opaque fitted sheet at `entities/mannequin/textures/mannequin-skin.png`.
Its `run_arms_forward` interface is retained by the approved R2. The historical
maintenance builder checks the accepted export hash before attempting generation.

The Office cockroach uses an opaque 32² native derivative of the committed
shared white sheet at `entities/cockroach/textures/white_01.png`, embedded
unchanged by `tools/props/build_cockroach.py`. All UVs sample its centre;
brown vertex pigment and closed geometry supply the detail. `COLOR_0` is a
linear glTF channel; the material's linear `baseColorFactor`
`[0.2231106261, 0.1249638443, 0.0856918012, 1]` anchors that palette to the intended
dark-brown `#462412` shell under unit white light. Its nonmetal roughness `0.48`
provides a restrained cuticle response from the existing environmental lighting.
The 480-triangle,
25-joint GLB faces +Z, has a floor origin and includes idle/walk/run loops
with articulated tripod scurry. This is an ordinary fitted model sheet,
with clamp sampling, opaque alpha, no emission and no tiling.
These source files are embedded during export; runtime reads the GLBs.

`core:table_sideways` is the accepted oak table resting on its long edge.
Its tabletop normal faces -X and its legs extend toward +X; ordinary level yaw
still controls placement. The completed export has 428 triangles and size
`[0.75, 1.4, 0.8]`, with its original opaque 256² fitted artwork, clamp sampling
and non-emissive material. The completed-model ledger owns its exact hash and
measured bounds after the obsolete constructor was retired. It creates no raster
artwork or new UV layout.

The retained Home builders in `tools/props/parts/domestic_remade.py` preserve
existing atlas regions and tableware layouts. The completed bed and office-chair
exports retain their standalone 256² PNG sources beside their GLBs:
`shared/props/bed.png` (frame/mattress/linen/board) and
`environments/office/props/chair.png` (shell/pad/metal/dark). Their superseded
constructors were removed; the source artwork remains. The switch retains its
`toggle` clip and node names; its rocker hinge is now at `(0, 0.066, 0.022)`
metres, with a 0.35-second, 30-degree travel. Its back remains at `z = 0`.

The accepted Pool collection supersedes the earlier 256², four-cell procedural
remakes. Its shared 512² sixteen-cell material sheet and dedicated tile,
charcoal-slat, foliage and sign sheets retain their delivered fitted UV frames.
Exact per-model bounds and texture edges are recorded in `completed-models.json`;
the historical constructors and masters cannot overwrite these accepted exports.
The unchanged basin lane decal is 1024×32 RGBA (32:1),
with a transparent margin, and is placed at 32:1 on one flat basin floor.

The accepted rubber duck uses the collection's shared opaque fitted material
sheet. Its geometry, source origin and physical buoyancy remain separate
contracts; the historical `duck_remade.py` four-cell exporter is superseded.

### 8.7 Winter static snow kit

The October 15 Library delivery replaces 39 Winter exports and three Outdoors
trees with exact completed GLBs. Their shared opaque 512² fitted atlas is
`environments/winter/props/winter_materials.png`; all 42 embed the same PNG.
UV layout, normals, material groups, alpha and source pivots remain delivered.
`completed-models.json` records measured bounds and exact hashes. Historical
October 8 procedural constructors remain source history and cannot overwrite
completed models. Ground/wall sheets, ice traction and sky/weather contracts
remain unchanged.

Winter uses the existing surface/model classes, with no new shader or raster
contract. `winter:tex_snow_01` is an opaque, seamless 1024² floor sheet at
`environments/winter/textures/floors/snow_01.png`. Snow and packed snow share
this source; their world-aligned repeats are 8 m and 5 m respectively. The
packed material uses a restrained compaction tint. Both are matte and carry
no normal map, emission or baked directional illumination.

The completed snow shells are closed and outward-wound. Evergreen bark,
foliage and snow use separate opaque, non-emissive material groups. The two
snow loads retain 3.2 × 6.8 × 3.2 m bounds and narrow trunk colliders; their
geometry is no longer required to prefix a different bare tree's vertices.
Exact-hash importer exceptions cover Outdoors trees 01/02 at 10,326/11,916
triangles and snow trees 01/02 at 7,982/9,958. Ordinary imports retain the
6,000-triangle ceiling. Snow on rocks and walls changes the visual envelope;
colliders cover their solid underlying bodies.

Large static models above the ordinary 1,500-triangle art budget use
prepared irradiance probes instead of a padded atlas receiver for every small
face. The five delivered tree variants currently meet this policy. Source geometry, normals, UVs and opaque shadow
casting remain intact. Source albedo stays unlit during transport, then receives
the solved directional field before packaging. A valid dark probe stays dark;
missing coverage uses the established environment-lighting fallback. Other
models keep their physical atlas density and the atlas page budget is unchanged.

Current rail-only straight/corner snow modules and the two linked fence
section/post modules retain their newer geometry, original PNGs and stock
contracts. The delivered integral-post railings are separate completed assets,
`winter:railing_snow_straight_assembly` and
`winter:railing_snow_corner_assembly`; do not substitute them into linked runs.

Modular cap origins are their base-contact plane, centred in X/Z. Sink caps
8–9 mm into their supports to avoid coplanar seams. Icicle origins are the
lowest tip, with all roots at the catalogue height; mount the root plane
15 mm into the underside. Prop Y remains floor-relative. Catalog sizes and
completed-model bounds define module dimensions. `tests.test_winter_assets`
protects sealed topology, UVs, mounting, material roles and retained modular
stock; generic completed-model checks protect accepted export identity.

Winter frozen-water albedo uses the existing **floor surface sheet** contract:
`winter:tex_ice_01` at `environments/winter/textures/floors/ice_01.png` is a
square, opaque 1024² PNG, seamless on both axes, world `(x,z)` UVs at an 8 m
repeat, north (-Z) at image top. Its blue-gray cloudy facets, sparse angular
cracks and pale marks are albedo artwork with neutral lighting. The material
supplies `alpha_mode: "blend"`, opacity 0.84, modest specular/shine (0.38),
no normal/emission maps and no reflection image. A darker opaque material reuses
this PNG on a shallow non-solid backing. `ground_surface: "ice"` controls
traction independently of the PNG's opaque alpha and the material's visual sheen.
Its omitted `ground_friction` resolves to 0.025 for rigid items while the player's
established ice movement remains selected by `ground_surface`.
Validate with the standard texture budget/seam checks and native pond views.

### Winter string lights

The three completed `winter:string_lights_{short,medium,long}` props share the
opaque 512² `winter_materials.png` atlas. Glass uses the separate amber
`winter_bulb_emitter` material; cable, sockets and opaque metal have zero emission.
No billboard halo, alpha blend or generated raster is needed.

Attachment centres span 2.8/4.0/6.6 m along local X, with .15/.28/.40 m sag.
Their origin is the lowest bulb underside, centered X/Z; +Z is front. The
completed spans contain 5/7/11 bulbs. The map builder uses the measured emitter
table in `tools/props/string_lights.py`, placing one point 2 cm below each bulb's
minimum Y, outside its opaque glass. It preserves each span's previous total
authored light power. Model emission alone does not illuminate the environment.
The timber lantern's front source is `[0, 2.56, .167062]`, clear of the frame;
only its `winter_bulb_emitter` slot emits.

### 8.8 Model geometry conventions (for context)

Linked rail-only sections keep a **floor-reference origin at Y=0**, with
intentional open space below their stock. `outdoor:porch_railing_straight`
and `outdoor:porch_railing_corner` (and their Winter snow variants) start at
Y=0.14; their top rail remains Y=0.95..1.05. `outdoor:fence_two_rail_section`
and its Winter snow variant start at Y=0.345, with two original timber rails
at Y=0.345..0.455 and 0.735..0.845. Their catalog height is measured from
the floor reference to the visual top, rather than the occupied-mesh height.
Only these documented sections use the exporter `floor_clearance` exception;
their separate support posts retain real base contact at Y=0. Do not refit
their occupied bounds to the full catalog height.

The timber section and newel are selected from the original
`fence_two_rail.glb` vertex/UV/color records, share its committed
`fence_two_rail.png`, and have one `fence_timber` slot (0). Their Snow copies
retain those records and add `snow` slot 1. The 1.78 m section's X endpoints
are ±0.89; the separate newel has a 0.12 m shaft, 0.145 m foot and 0.16 m
cap at Y=1.05. Snow seats 8 mm into both rails/caps and changes visual
envelopes only. Use one shared upright per linked endpoint or corner.

The repaired porch corner remains centred in its 0.30 m catalog envelope:
its post anchor is local X/Z `[-0.09,-0.09]` and its two join endpoints are
`[0.15,-0.09]` and `[-0.09,0.15]`. Its timber and metal slots remain 0 and
1, with snow at slot 2. The end's newel is local X=0.09 and its rail meets
an adjacent section at X=-0.15. Preserve these anchors instead of inferring
them from a bounding-box edge.

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

### 8.9 Home concept furniture and static domestic fittings

The October 2026 Home reconstruction uses the existing prop class, with no
new runtime image type. Its authoritative sheets remain immutable:
`assets/references/home/Home Environment Asset Sheet.png` and
`Places_ Warm Low-Poly Home Asset Sheet.png`. The former establishes cream
upholstery and a practical kitchen; the latter adds the low timber console,
shelf coffee table, checked cushions, books, mug and sunset CRT programme.

All new Home models retain a square 1024² `*_master.png` and one square,
opaque 256² native atlas beside their GLB. The master is downsampled once with
Lanczos by `tools/textures/author_home.py`. GLBs embed the native image;
ordinary prop builds load it rather than paint replacements. High keeps that
native atlas; Medium uploads 192² and Low uses the existing 128² policy.

The four regions are row-major quadrants. Cabinets use body/door/counter/
metal; sofas use body/seat/back/wood; other new static fittings use main/
secondary/detail/dark. The CRT keeps screen/bezel/body/panel and fits its
screen image in the upper 75% of the screen quadrant. Its standalone city
art is retained under `props/art/city_dusk_01.png`; it is screen artwork, not
a world sky or an exterior backdrop. The opal globe retains orb/cord/rose
quadrants and its ceiling-contact top at 0.8 m; its revised 380 mm diameter
is recorded in the catalog. Switch node names and the toggle clip are
unchanged. Tableware, floor-plant and bed imagery/geometry are retained.

Models face +Z, are horizontally centred and have their contact plane at
y=0. Wall outlets and small landscapes have real stock depth; place their
back on the wall face and rotate +Z toward the room. Cabinet doors use real
recesses and thin construction joints. Lamps/strip diffusers use GLB emissive
groups, while their illumination is authored as ordinary map-owned lights.
All new fitted art is opaque and clamped, with UVs within 0..1. The ordinary
1500-triangle art limit applies; every rebuilt/new Home model is below 800.

The backsplash is an ordinary 1024² opaque periodic wall surface: eight
15 cm tiles in a 1.2 m world repeat. The revised floor tile has four 30 cm
tiles in the same repeat. Oak remains eight 20 cm boards over 1.6 m; walnut
has eight 15 cm boards over 1.2 m. Patterned paper has restrained sage sprigs
on a 25 cm cell. `wall_paint_warm_01`, `hardwood_oak_warm_01`,
`ceiling_warm_01` and `ceiling_plaster_warm_01` are local Home variants;
the earlier shared paint/oak/ceiling sources remain byte-identical.
Shared cedar, door-paint, timber-trim and flush-fixture
sources remain intact. Validation uses the standard catalog, source/embedded
pixel match, prop bounds/UV/topology, surface seam and native-render checks.

Stage 6 adds `home:dining_chair_refined` as a reusable optional chair. Its
324 triangles and 752 exported vertices replace the original chair's 204/432
for the refined hero only. Selective seat corners, rail bevels and softened
slat tops retain the exact 0.50 × 0.902 × 0.49 m local bounds, floor-contact
origin, +Z orientation and opaque 256² `dining_chair.png` atlas. The new GLB
embeds that existing PNG; no master or runtime image is resized or replaced.
Geometric flat normals follow the established exporter/importer contract;
no normal-map/tangent feature is introduced. The original model and builder
remain byte-identical for prepared maps awaiting Stage 7 adoption.
[Audits and measured hero costs](art-style/stage6/README.md) record topology,
UVs, collision and native acceptance.

### 8.10 Outdoors concept reconstruction

The bare Outdoors kit uses the existing static-prop contract, with no new
asset class. Three trees are closed opaque faceted foliage/branch meshes;
they keep floor-contact origins, +Z forward and trunk-only map colliders.
`tree_02` now has a 2.0 × 6.2 × 2.0 m slender envelope. Tree artwork and all
new static fittings use four fitted quadrants: top-left bark/metal/mineral
body, top-right stock/pane, bottom-left foliage/roof, bottom-right alternate
foliage/stone. These are complete geometry/UV rebuilds; the old cutout-tree
UVs are not combined with the replacement sheets. No alpha blending is
introduced. The retained grass cards keep their original two cells and MASK
alpha, loaded from their committed PNG.

Masters are 1024² and native model derivatives 256² through Lanczos.
The original wall-module sheet `house_base` retains siding/trim/glass/jamb
quadrants. Family 02 keeps its seven-region normalized layout exactly, with
1024² `house_02_materials` and 128²/256² derivatives. Warm window panes use
only their own emissive primitive; framing remains opaque unlit stock.
The other four house families retain their original source/GLB artwork.
Normal house builders load the actual fitted family PNGs.

The lantern family has tapered four-pane heads, separate corner bars,
pyramidal hoods and finials. Existing placement envelopes and light offsets
remain. Pane emission does not add environmental illumination. Lamp
placements with an owned light must set `occludes: false` on the fixture,
because the coarse closed head otherwise encloses the emitter anchor. Eight new
static families are `bush_round`, `bush_low`, `fence_two_rail`,
`masonry_pier`, `porch_canopy`, `road_barrier`, `boundary_ridge` and
`campfire_static`. The static fire retains the original campfire's geometry,
materials and committed `showcase_surface.png`, with no animation clips or
character ownership; its animated source remains unchanged.
The canopy high edge faces local −Z; the pier's cap at 1.2 m supports the
existing fence lantern; ridge geometry is a visual boundary, not collision.

Grass, gravel and concrete keep 2.0/1.6/2.0 m repeats and opaque square
512² production sheets, now derived from retained 1024² masters. Concrete
joints form 1 m slabs. Dirt contains 2–6 cm chips; broad lawn clusters carry
variation into mips. Path feathers retain 2:1 edge and square end/corner
roles, blended alpha, original direction and 256×128/128² native sheets;
masters are 1024×512/512². Normal texture builds load committed images.

At the end of the serial Outdoors pass, Winter's snow variants retained the
preceding canonical bare evergreen/rock geometry and textures. The October 8
Winter reconstruction (§8.7) now reconciles the current bare kit and regenerates
supported snow variants; the canonical identity/support checks pass. Original
complete bases remain in preserved review copies and preceding commits.
`tree_03_snow_base.png` remains the exact prior evergreen atlas for historical
evidence. No Outdoors source file is changed by that Winter reconciliation.


### 8.11 Beach concept reconstruction

The Beach kit keeps existing asset classes and importer behavior. Its 27
static models use metres, +Y up, +Z forward, horizontally centred floor-contact
origins and opaque closed faceted geometry. Their fitted shared atlas retains
a fixed 4×4 layout: sand/grass/rock/stucco, wood/plank/palmtrunk/palmleaf,
blue/coral/yellow/ivory, dark/turquoise/window/foam. The 1024² master and exact
256² Lanczos native derivative live in `environments/beach/props/`;
every GLB embeds the native PNG unchanged. Pale town coral is model vertex
tint over ivory; umbrella sectors and chair upholstery use their fitted cells.
Flat geometric normals are intentional under the established importer fallback.
There are no new tangent/normal-map or emissive contracts.

Seven labelled surface families plus grass, yellow roof and foam have opaque
1024² square masters and seamless 512² runtime PNGs under the Beach texture
subtree. Opposite edges agree exactly. They use broad clean material marks,
without baked directional lighting. The shallow/deep water definitions share
one polygon-cell PNG; BLEND coverage belongs to the existing material/WaterDef
workflow, not image alpha. The sky is opaque seamless 2048×1024 equirectangular
art, with +Y zenith at V=0, −Z at U=.5, sun bearing/elevation +45°/+45° and
no baked landscape. Its matching authoring light points toward [−.5, −√.5, .5].

Three animal rigs live in their own `entities/beach_<animal>/` subtrees. Each
owns an opaque 1024² source and fitted 256² box-average derivative, embedded
unchanged in its GLB. Their five looping clips preserve the runtime bind
half-extent ×1.15 +.05 m culling and lighting envelopes at every sampled pose.
Explicit `play_animation` actions initialize idle/fly, idle/walk and swim;
`animation.playing` alone is insufficient. See the per-animal READMEs for
measured dimensions, UV regions, pose bounds and floor-relative placement.

Normal builds load committed PNGs. `tools/textures/beach_art.py --author` and
animal `--author-textures` are deliberate offline authoring commands. Their
`--check` modes reproduce both sources and derivatives without replacing them.
[Beach kit](assets/ENVIRONMENTS.md#beach) records inventory and support
helpers. Decorative terrain, hollow structures and dock GLBs do not create
walking collision; the map owner must install the separate authored support.

### 8.12 Frutiger Aero concept reconstruction

The 23 static modules retain metres, +Y up, +Z front, centred horizontal bounds
and floor-contact origins. Their closed, deliberately faceted geometry uses
the existing model import and lighting contracts. Hollow glazing uses explicit
BLEND scalar opacity; framing remains opaque, and the embedded PNGs remain
alpha 255. Separate fitted structural stock supplies map collision for openings,
furniture, curved boundaries and the shallow basin. Never collide a hollow
module's entire visual bounds.

The shared 256² native atlas retains its 1024² master: sixteen palette cells
in the top quarter, four branding panels in the middle half, and accent/leaf/
ring/terminal artwork in the bottom quarter. Five dedicated atlas/master pairs
(`accent_panel_atlas`, `kiosk_atlas`, `banner_atrium`, `banner_corridor`,
`banner_reception`) preserve 1:2 artwork in their left 128×256 native / 512×1024
master region, with white/lime/cyan/silver stock swatches on the right. All are
opaque, top-down, +Z upright, UV-clamped, and Lanczos derived from 1024² masters.
Do not repack these regions without updating their model UVs.

Four periodic 512² surfaces retain 1024² masters. Cyan glass has straight alpha
52–64; white panel, aqua tile and cyan water are opaque. Five fitted RGBA decals
retain transparent margins: atrium/reception/kiosk are 512×1024, corridor/leaf
512². The opaque 2048×1024 U-periodic equirectangular sky has its sun at bearing/
elevation +45°/+45°, matched by the authored directional light. Normal builds
load committed PNGs; `tools/textures/frutiger_aero_art.py --check` reproduces all
26 source/native images without writing. The kit adds no depicted creature or
locomotion animation and uses the existing water presentation.

[Aero kit](assets/ENVIRONMENTS.md#frutiger-aero) records the complete
coverage and collision/UV contracts.

### 8.13 Birthday party Blender assets

Party furniture, printed paper, gifts, cake, latex balloons and tabletop weights use the existing
fitted prop/entity class. Editable `.blend` sources and exported GLBs live under
`environments/arcade/party/`; each embedded PNG has an equivalent standalone
source there. Printed artwork retains1024-square masters. Four cloth patterns use 1024-square
native PNGs across eight rectangle/round variants; six gift wraps use dedicated
512-square natives. Cups, plates and hats retain128-square sheets and the family
palette retains32-square. High uploads these authored prints unchanged; Medium
and Low intentionally reduce prop images to 192 and 128. These exceptions preserve
fine print strokes at actual model scale. The round skirt unwrap follows its
vertical drape length rather than projecting it into a narrow radial strip.

Seven numeric RGB ink masks retain1024-square masters and256-square natives.
Each shares the print's normalized UV frame/orientation; black ordinary backing
and white printed ink give `extras.uv_response=0.65` through the existing
`extras.uv_response_mask` texture binding. Printed stripes can fill the wrap.
Ribbons/bows remain nonreactive. These masks add 3.5 MiB across 14 model payloads;
together with larger prints, the pack audit reports 89.434 MiB of unique PNG-family images within its 128 MiB
limit. Actual per-model copies remain counted. All sheets are opaque, clamped,
non-tiling, with finite normalized UVs and +Y-up metre geometry. These printed
surfaces supply no ordinary emission or illumination. Only tiny candle-flame
material groups emit; their separate attached light supplies illumination.

Every gift's underlying box is the same 0.30 m cube at scale 1. Ribbons and bows
may extend beyond the box. Cloths are separate thin draped shells whose lowest
hem is local Y=0; compose their published top height with the supporting table.
The balloon source has a neutral glossy latex body, narrowed neck and knot,
and named bounded string segments. Its runtime colour changes latex albedo,
while its attachment remains fixed and the string follows simulation-time motion.
Bouquet weights are separate ordinary opaque fitted props; strings attach to
the published loop height above their contact origin, clear of tabletop cloth.

Catalog entries may declare offline `authoring: {"tool": "blender", "source":
"<asset-relative>.blend"}` ownership. `tools/props/build.py` verifies both source
and GLB exist within `assets/` and preserves these exports even with `--force`.
The ordinary `--check`, asset audit and Rust model tests still inspect the GLBs;
this ownership record does not exempt geometry, UV, PNG, scale or budget rules.

### 8.14 Arcade soda machine revision

The concept-directed Bepsi rebuild uses the existing fitted model class: opaque,
clamped 256-square native PNG, retained 1024-square master, +Y up, +Z front,
floor-contact origin and a 0.96 × 2.03 × 0.86 m catalog footprint. Its rebuilt
geometry and UVs are published together. This replaces the earlier STRIKE layout;
the old atlas must never be applied to the revised GLB.

Native atlas rectangles (x, y, width, height) are logo `(0,0,128,80)`, twelve
drink labels in a four-column/three-row grid of 32-square cells starting at
`(0,80)`, lower artwork `(0,176,128,80)`, central payment strip `(128,0,64,192)`,
delivery back `(128,192,64,32)`, and body/trim/dark/metal in the four 64-square
cells at X=192. Lower door sections share one continuous UV frame around a real
delivery aperture. Only advertisement/selection faces and restrained lower
backlighting emit; cabinet stock and payment/delivery hardware remain ordinary.
The retained generated illustration is an offline input, never a concept-board
crop. `author_retro_arcade.py --only vending_machine` fits the source PNG and
`build.py --only arcade:vending_machine` rebuilds the corresponding GLB.
`inspect_arcade_vending.py` is an explicit installed-Blender preview tool; the
Python builder remains authoritative and no Blender/map bake enters default tests.

### 8.15 Arcade commercial restroom kit

The reusable restroom fittings use the existing opaque fitted model class,
with +Y up, +Z front, metre scale, centred X/Z and base-contact Y=0. Editable
Blender source, GLBs and measured metadata live in `environments/arcade/restroom/`.
The completed exports and editable Blender source are the maintained assets;
the superseded one-off authorer was removed. Normal builds preserve the exports
through catalog `authoring` ownership and validate their accepted hashes.

All fittings embed the unchanged opaque 32-square `party_palette.png` from
`environments/arcade/party/models/`. UVs sample the centres of its existing
row-major four-by-four swatches (white, cream, metal and charcoal); material
factors give muted teal laminate and restrained porcelain/chrome response.
No atlas is repacked, resized, or synthesized at load. Each model's decoded
copy is still charged to the ordinary 128 MiB pack budget. Geometry supplies
real bowl cavities, seat rings, trap/faucet stock, partition thickness, foot
plates, hinges and latches. The mirror-frame model is an open frame: the map's
opaque planar opening pane supplies the actual reflection, with no painted image.

The room reuses unchanged 1024-square Home floor tile at a 1.2 m repeat
(30 cm tile) and Pool wall tile at 2 m (20 cm tile), plus the existing
128-square fitted ceiling-grille decal. They retain their original tiling,
alpha, orientation and resolution contracts. Restroom tile trim uses ordinary
wall projection and is never ultraviolet carpet trim. Playable entry and stall
leaves use the interactive `restroom` door kind: flat 42 mm laminate panels,
small metal latches and narrow jambs, with existing PNG material references.
They start closed and swing into the room/bays. The kit's standalone leaf GLB
remains a reusable static construction piece; it is not the playable collider.

### 8.16 Arcade Bepsi bottle label

Both upright `soda_bottle` and `litter_bottle_fallen` embed the same opaque
256-square `props/soda_bottle.png`, derived from its retained 1024-square
master by the explicit offline `tools/props/author_bepsi_bottle.py`. It fits the
existing production vending globe/word artwork; vending sources are unchanged.
Native UV rectangles are label `(0,0,256,128)`, bottle `(0,128,128,128)` and
blue cap `(128,128,128,128)`. U traverses the label circumference once; V runs
upright from shoulder to base. Three upright logo repeats preserve readable
branding around the 12-sided bottle. The same normalized frame is embedded in
both exports; the fallen bottle rotates geometry after fitting the upright UVs.
The ordinary blue printed label and cap remain non-emissive and non-reactive;
only the existing glass slot uses BLEND opacity 0.87. All images are clamped,
non-tiling, with normalized UVs. Model dimensions and collision are unchanged.

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
    High ≤512, Medium ≤192, Low ≤128.
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

### 9.1 Ultraviolet-reactive artwork

The Arcade extension introduces a separate fluorescence response. Here
"ultraviolet" describes illumination, while "texture UVs" describe image
coordinates. Catalogue material fields `uv_response` (finite 0..2, default 0)
and `uv_response_mask` select the response. A mask is an opaque numeric RGB
PNG, sampled as linear data using the albedo's texture UVs. Black backing must
remain black in the mask; white fluorescent motifs react fully, and grey
edges can represent partial pile coverage. Alpha is ignored. Masks use the
512/192/128 High/Medium/Low numeric-mask budgets and the same wrapping as
their owning surface or fitted model texture. They never use
`emissive_mask`, enter the emission-only bloom image, or illuminate neighbours.

Source-level fixtures author `ultraviolet` independently of visible RGB
brightness. Visibility, facing, range falloff and fixture enabled state govern
excitation. The bounded implementation supports four ultraviolet fixtures,
including initially disabled ones. The response remains independent of the
selected lighting quality. Model materials use glTF extras `uv_response`
and `uv_response_mask` with an embedded glTF texture index and `texCoord: 0`;
the identical standalone mask PNG remains under `assets/`.

`arcade:blacklight_tube` is an operational narrow ceiling batten in the existing
fixture pipeline. Its fitted opaque lens is 8:1, horizontal u along the tube;
dark housing stays nonemissive. A retained 1024×128 master produces the
256×32 native face. Its visible purple emission is restrained and independent
of ultraviolet intensity; switching the fixture disables both its face and
its ultraviolet influence.

The Arcade carpet family deliberately retains a 1024-square master and a
256-square native albedo and co-registered mask. Six distinct designs use a
4 m world repeat: Cosmic Bowl, Neon Confetti, Starburst, Retro Swirl,
Geometric and Arcade Grid. Most of each tile remains dark navy; broad motif
outlines must survive 256-square sampling and a 128-square QA preview. The
current Surface budget keeps these native albedos at 256 on Low; the
128-square preview is a mip stress check, not a claimed Low upload size.

All Arcade model atlases are fitted square 1024-square retained masters with
128-square palette-only native PNGs or 256-square graphic native PNGs. The
palette class covers pins, balls, chairs, table, return, rack, batten, plush,
gutter and divider; detailed cabinets, CRTs, vending, pinball and labelled
small props retain 256-square natives. These existing atlases retain their resolution; the current 128 MiB
decoded prop-pack budget includes detailed party prints and their ink masks. Their model-defined regions must be recorded by the
builder and changed together with model texture UVs. Acoustic ceiling sheets
are square repeats containing two 2:1 modules; visible fluorescent diffuser
faces remain actual 2:1 PNGs (1024×512 master, 256×128 native). The bowling
mural is a fitted 4:1 RGBA decal (1024×256) with transparent outer margins;
each level placement must retain that ratio and stay within the 10 m decal
edge limit. Reference boards remain unchanged source records and are never
cropped into production textures.

The five upright cabinet atlases retain their explicit native-pixel regions:
4:3 screen `(0,0,128,96)`, 4:1 marquee `(0,96,128,32)`, 2:1 control deck
`(0,128,128,64)`, square coin door `(0,192,64,64)`, body/metal/dark cells
`(64,192,32,32)`, `(96,192,32,32)`, `(64,224,32,32)`, and 1:2 printed side
`(128,0,128,256)`. The existing pale hardware cell `(96,224,32,32)` now
contains four 16-square fitted swatches, in reading order: red, amber, cyan,
ivory. Cabinet UVs and regenerated GLBs publish this subdivision together.
Opaque RGB original cabinet illustration inputs live in
`environments/arcade/props/illustrations/cabinet_<kind>_side.png`; these are
flat, text-free 1:2 portrait print sources, at least 512×1024, with a 2048-pixel
source-only edge ceiling. They are deliberately fitted into the existing 1:2
side region of the 1024-square master before its Lanczos 256-square derivative
is embedded. They are never loaded at runtime or registered as surface sheets.
Original gameplay inputs beside them use `cabinet_<kind>_screen.png`: opaque
RGB, 4:3, at least 512×384 with the same 2048-pixel source-only edge ceiling.
They fit the existing 128×96 native screen region, with deterministic score,
lap/round/health and credit text composed in the retained master. No generated
reference-board pixels, frame, physical monitor or baked scanlines enter them.
Printed body, side, coin door and controls remain nonemissive; only CRT glass
and the marquee face emit. The sci-fi cabinet preserves its co-registered
256-square numeric ultraviolet mask and 1024-square master; that pigment
response remains independent of emission. `author_retro_arcade.py --only
cabinet_<kind>` authors the saved sheets; the normal selected `build.py` export
reads those PNGs and validates closed geometry, UVs, dimensions and its stricter
800-triangle family ceiling. Native renderer captures judge artwork, controls
and silhouette under the actual Arcade lighting.

---

## 10. Normal maps

* A normal map is an ordinary PNG in the asset tree that a material names with
  `normal_texture` (plus `normal_strength`, `0.0..=2.0`).
* It is a **surface-class** texture for quality purposes: High 1024, Medium 384, Low 256.
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

### Editor catalog previews (0.13.0)

`assets/editor/previews/*.png` are opaque 160×120 RGBA8, 4:3 catalog
previews. They are offline renders of the actual shipped GLB's geometry,
materials, texture and catalog tint, or fitted samples of the actual material
texture. They retain their source orientation and never replace world textures.
`tools/editor/build_previews.py` writes the catalog-ID index and source hashes;
run it explicitly after changing a model/material. `--missing-materials` adds only
absent material swatches and preserves every existing/native preview and index
record; this is the bounded path for filling a picker gap. The editor only reads these
committed PNGs, decodes visible tiles lazily and retains at most 24 tiles.
Missing previews show an explicit message and keep selection available.
Native fixture bodies and the three built-in door styles use the same 160×120
contract under `assets/editor/native-previews/`, with capture/source metadata in
`native-previews.json`. Capture these explicitly through the native editor after
changing their runtime geometry, mounting or textures; the generator preserves
them rather than substituting flat fixture faces. Editor-enabled packages include
this directory and its index; player-only exports do not need it.

### 12.1 Application icon and documentation screenshots

* `assets/shared/branding/places.png` is the single approved icon and README
  image, formerly duplicated as root `icon.png` and `logo.png`. Keep its original
  square composition, orientation and opaque RGB bytes unchanged. New icon
  sources use RGBA, square, non-interlaced PNG at ≤512 px. The existing icon
  contract is checked by `tests/test_package.py`; it is non-runtime artwork.
* `docs/screenshots/*.png`: 960×544 documentation captures. Not runtime
  assets; no contract beyond being still frames.
* `docs/images/item-physics-0.10.0/*.png`: unchanged native gameplay
  captures, 1280×720 or 2560×1440 RGB/RGBA. Retain full frame, orientation
  and colour; these are diagnostic evidence, outside the runtime catalog.
* `docs/images/modular-railings/*.png`: unchanged 1600×900 native gameplay
  captures. Retain full frame, orientation and colour; no runtime catalog entry.
* `docs/images/flashlight-0.11.0/*.png`: unchanged 1512×838 native gameplay
  captures. Retain full frame, orientation and colour; diagnostic evidence only,
  outside the runtime catalog.
* `docs/images/developer-console-0.11.0/*.png`: unchanged 3024×1898 native
  gameplay captures. Retain full frame, orientation and colour; diagnostic
  evidence only, outside the runtime catalog.
* `docs/images/sound-0.11.0/*.png`: unchanged native full-frame captures at the
  recorded drawable size. `docs/videos/sound-0.11.0-*.mp4` retains 1280×720 native
  frames in order and actual playback-output PCM, encoded H.264/YUV420P with
  limited-range BT.709 metadata and AAC. Variable
  presentation times follow the captured audio clock; no microphone input or
  replacement soundtrack. Both are diagnostic evidence outside the runtime catalog.
* `docs/videos/places-0.9.0.mp4`: non-runtime promotional gameplay footage,
  16:9, native source at least 960×540 with 960×540 delivery at 30 fps,
  H.264/YUV420P with fast-start
  metadata. Capture real held player controls from current installed packages;
  preserve frame chronology, camera orientation and composition within each
  segment. Use simple cuts between environments, with no synthetic scenes.
* Its thumbnail reuses an unchanged native PNG in `docs/images/release-0.9.0/`.
  A compact animated lossless WebP preview samples the original native RGB
  frames at 384×216 and 6 fps; retain the full colour range, 16:9 orientation
  and actual motion. Never derive it from a palette-reduced GIF. These derivatives
  are outside the runtime catalog and never replace game textures.

### 12.2 Shared untextured white sheet

`core:tex_white_01` → `assets/shared/textures/white_01.png` is the renderer's
neutral fallback sheet: a flat opaque white fill. Untextured fixture bodies,
plain geometry and every texture slot with nothing better to bind sample it
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

Fluorescent panel housings use the existing metal surface material instead of
this sheet (§6). The white sheet itself is not an authoring target.

### 12.3 Internal image resources

The fixed game sheets are ordinary catalogued PNGs. The editor-only font is
embedded by the editor renderer and deliberately excluded from the world catalog,
so changing editor typography does not invalidate compiled map dependencies. All
are embedded
from their repository files so a broken external asset or an installation
without an asset directory can still display the UI and diagnostic fallback.
Their decoded RGBA pixels are pinned by tests against the reviewed sheets;
changing compression is safe, but changing layout or orientation is not.

| Image | Catalog id and repository path | Contract |
|---|---|---|
| 256×128 HUD font/hand atlas | `core:tex_ui_font_01` → `assets/shared/ui/font_01.png` | **2:1** RGBA8 sheet, top-down. Original 128×64 ASCII block retains every 8×8 glyph and white-cell pixel at the upper left. Glyph/white-cell UVs use the new sheet dimensions. ASCII 32–126 retains sixteen cells per glyph row; cell zero stays opaque white. An upright 64×64 hand cell occupies pixels `(0,64)..(64,128)` in the formerly unused lower-left area. Thumb points left; hand interior/exterior are alpha zero with antialiased off-white ink and a thin dark edge. No baked key or badge, no tiling. Other unused cells stay transparent. The catalog's historical 128×64 description names the logical grid; its bytes stay unchanged for package compatibility. |
| Editor font atlas | `assets/shared/ui/editor_font.png` | **8:7**, 512×448 RGBA8, top-down, 16 columns × 8 rows of 32×56 cells. ASCII 32–126, cell zero solid white; unused cells transparent. Roboto Mono medium, genuine antialiased outlines, OFL source and explicit offline builder under `tools/fonts/`. Editor-only clamped linear sampler, no mipmaps, independent of game texture quality. Rectangles/viewport guides sample inside the white first cell. No runtime rasterization. |
| 256×256 validation decal atlas | `core:decal_test_01` → `assets/shared/decals/validation_atlas_01.png` | Fixed square RGBA8 sheet; four 128×128 cells, eight-pixel UV gutters, only slot zero contains the existing frame and text. The PNG retains the atlas's bottom-up row convention, so its text appears vertically inverted in an ordinary image viewer. Preserve that orientation and alpha-zero background; the existing atlas UVs make it upright in game. No tiling. |
| 64×64 missing-texture checker | `core:tex_missing` → `assets/shared/textures/missing_01.png` | Fixed square opaque RGBA8 sheet; eight-pixel cells alternate `[255, 0, 255, 255]` and `[24, 24, 24, 255]`, starting magenta at the top-left. Preserve its symmetric orientation and checker period. |
| 2×2 emergency white sheet | `core:tex_white_fallback_01` → `assets/shared/textures/white_fallback_01.png` | Fixed square opaque RGBA8 sheet, every channel 255. Used only if the embedded primary white sheet cannot decode. No tiling or authored UV detail. |

These layouts are engine resources rather than map authoring targets. The
existing atlas accessors and decal material slot remain unchanged. The shared
decode accepts only a closed set of embedded repository files; failure is a
repository build invariant, covered by complete pixel hashes and dimension
checks. External images continue to use fallible decoding and the checker.

The HUD uploads the committed atlas once and renders the hand as one textured
quad in the existing UI draw. The 2:1 sheet retains
every original glyph/white-cell pixel; shared accessors adjust their UVs for the
new dimensions. The additional decoded/uploaded storage is 96 KiB, independent
of world texture quality, with no mipmaps or per-frame texture work. The hand cell
recreates the rounded outline from the user-provided Desktop reference
(`6901637caf3b8791f7d3546ceee9f8543c136759544ff62b6d35e9bba12467e0`).
The current bound interaction key is drawn separately in off-white inside the
transparent palm, with a subtle dark text edge. Long key names fit within that
same palm without stretching the silhouette. The icon stays low and compact;
visibility follows the precise eligible aim target and disappears throughout
holding, menus and console use. No image is generated at runtime.

Computed lighting images are derived from level geometry and light settings:

| Image | Producer | Purpose |
|---|---|---|
| Lightmap atlas pages | `src/engine/lighting/lightmap/` + offline compiler | Incident linear HDR and signed moment pairs, prepared into compact PLLC1 blobs (legacy/exact KTX2 remains readable), expanding to the same linear half-float GPU planes. Full plans up to fourteen pages per contribution group; lower profiles retain eight. The player uploads the exact produced layers, subject to the measured 320 MiB +64 KiB typed record bound and unchanged switch-group/aggregate/shape guards. PNG dumps are diagnostics, not authored assets. |
| Reflection probe cubemaps | `src/engine/render/common/reflections.rs` (routing) and `src/engine/render/wgpu/reflections.rs` (offline capture) | Packaged HDR probe captures and roughness mip chains, uploaded at level load; the player does not recapture them. |

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
* the shared caustic intensity mask (§7.1b).

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
4th line and every 8th position. Alpha is never measured. The Python tool
checks both profiles for all nineteen catalog environment sheets.

Repository gates that run the seam tool automatically:

* `python3 -m unittest tests.test_package` runs `--check` over every
  environment-class texture in the catalog (the six office sheets, four pool
  sheets, three glass sheets, linoleum, metal, plastic, grille and the two
  normal maps).

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
| Surface sheet | 1024 | 384 | 256 |
| Fixture face | 1024 | 384 | 256 |
| Decal sheet | 1024 | 384 | 256 |
| Prop sheet (embedded) | 1024 | 192 | 128 |
| Emissive mask | 512 | 192 | 128 |
| Sky panorama | 2048 | 768 | 512 |
| Lighting | Independent Static / Full | Same | Same |
| Reflections | Independent Off / Medium / Full; suppressed in Static | Same | Same |

New levels ship Static (`off`) and Full prepared variants. Static retains vertex
lighting; Full retains the full bake at every texture quality. Legacy Medium
packages remain readable and saved Medium lighting becomes Full. All texture
qualities retain authored presentation defaults (exposure 1, knee 0.75,
saturation 1.03, contrast 1.02).

* Downscaling happens **once, at upload / level-load time**, through a
  proportional box filter. The longest edge reaches its budget and the other
  edge scales by the same ratio, rounded up. Numeric images average integer
  source regions; colour images use the same regions in linear light with
  coverage-weighted RGB. Source POT requirements do not constrain runtime
  dimensions or change the normalized UV frame.
* The result is cached with the texture; nothing is rescaled per frame.
* **High is the native presentation.** A 256×256 prop sheet and a 1024×1024
  surface upload unchanged; High never resamples an asset that already sits
  within its class budget.
* Medium and Low are optional quality/performance reductions, not a hardware
  requirement: Medium limits surface sheets to 384 and props to 192;
  Low limits surfaces to 256 and props to 128. A native 256² prop atlas therefore
  has three distinct uploads: High 256², Medium 192², Low 128². They must never
  dictate the size of the asset stored in the repository.
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
384 px image and Low its 256 px image from the same source. Replacing a
1024×1024 sheet with a 256×256 sheet would visibly lower High quality.

Ordinary prop sheets retain 256×256 or smaller native sizes. Reviewed party
cloth and wrapping prints use 1024 and 512 respectively, preserving original
print definition across metre-scale drapes and folded wrapping. High retains
accepted images up to 1024 unchanged; Medium and Low use their 192
and 128 budgets. The authoring policy and whole-pack budget bound these exceptions.

### 14.3 Hard limits

* **2048×1024 for sky panoramas only**, validated as 2:1 and POT before pixel allocation by the sky loader; at most 8 MiB decoded RGBA8. High preserves these pixels, Medium uploads 768×384 and Low 512×256.
* **1024 px per edge** for ordinary PNGs the decoder loads: surfaces, fixtures,
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
| Sky panorama | 2:1, POT, repeat U / clamp V | 2048×1024 | 2048 |

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
* ≤1024 px on either edge for ordinary sheets; sky panoramas ≤2048×1024.
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
| Colour space | Colour PNG RGB is sRGB; sampled through `Rgba8UnormSrgb`. Normal maps and emission masks are numeric `Rgba8Unorm`. The resource role decides interpretation; PNG gamma/ICC metadata is ignored. Lighting and material arithmetic use linear HDR. |
| Premultiplied alpha | not used; alpha is treated as straight coverage |
| Metadata | ignored (non-pixel chunks are not read; the decoder keeps only pixels) |

Authoring consequences:

* Paint albedo and emissive colour in ordinary sRGB. Do not paint a display
  curve, exposure correction or baked illumination into the source. Material
  factors, light colours and emission intensity are linear numeric values.
* RGB is sufficient for any sheet whose material is `opaque`: the decoder
  expands it with alpha 255. RGBA is only needed where alpha is actually read
  (`cutout`, `blend`, decal cut-outs).
* The shipped set is mixed: office surfaces are RGB, pool surfaces are RGBA
  with an unused alpha channel, core sheets are RGBA. All are valid. The
  channel choice is not a contract.

---

## 17. Texture filtering and wrapping

Stage 5 preserves every existing PNG/model/UV/alpha contract. Reusable catalogue
water and ice gain restrained probe reflection; snow gains broad weak specular
response. Prepared water charts and optional vertical-depth transmission are
renderer/level controls, not texture changes. See [Stage 5 contracts](art-style/stage5/contracts.md).

Filtering and wrapping are chosen by the **texture's role**, not by the asset.
Colour fitting and mip generation average decoded linear RGB with alpha-weighted
coverage, then store sRGB bytes; numeric maps use raw channel averages. Colour
odd-edge reductions include every source texel. Alpha remains straight coverage.
Texture-size budgets and the independent Texture Filtering setting do not
change when Lightmaps is changed:

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
| Reflection probe cubemap | `CLAMP_TO_EDGE` | prepared roughness mip chain | linear + explicit roughness LOD |

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

The world shader accumulates linear HDR:

```
linear_albedo = srgb_decode(texture.rgb) × vertex_color.rgb
linear_scene = linear_albedo × linear_light + sheen + reflection + emission
```

* Atlas vertices carry linear material tint and the legacy architectural face
  shade. Vertex-lit vertices additionally carry their linear lighting without
  byte quantization. Atlas/probe irradiance remains linear HDR until shading.
* The transport solver samples **linear albedo reflectance** for diffuse
  bounces. Architectural reflectance is tint times mean decoded texture; it
  excludes baked lighting and the artistic face shade. Model reflectance uses
  the primitive's linear base factor times its clamped sheet sample. Stored
  lightmap/probe irradiance excludes the receiving surface's albedo.
* Material emission is additive linear radiance: sRGB emissive colour sheets
  decode at sampling, RGB masks are numeric, and intensity multiplies before
  accumulation. Emission does not create a light; author a light separately.
* Scene, emission, blur and reflection resources use `Rgba16Float`. Authored
  presentation owns fixed exposure, a hue-preserving shoulder and sRGB encoding;
  its defaults are shared across quality (§14.1). No presentation curve is baked
  into illumination or texture data.
* Paint texture colour once. Where tint is authored, a pale neutral source
  remains useful; no tint leaves the full source colour. Do not compensate for
  the renderer with brighter assets or per-model light multipliers.

Low-poly mesh normal conventions: absent NORMAL means a flat geometric normal
per triangle, including the vertex-lit fallback. Explicit normals preserve the
asset's smooth/hard-edge choices. Normals use inverse-transpose transforms;
tangents use the ordinary transform and are orthogonalized against the normal.
Mirrored UVs carry handedness. Architectural normal maps use numeric tangent
space with the source UV frame. glTF normal-map textures are not yet imported.
See [the Stage 2 contract](art-style/stage2/contracts.md) for storage versions,
legacy behavior, sampling details and precise remaining limits.

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
* exceed the class ceiling: 1024 px for ordinary sheets, 2048×1024 for sky panoramas;
* invent an aspect ratio for a fixture from the current file's dimensions.

If no class covers the asset, **inspect the implementation and update this
specification before introducing the new asset.**

---

## 24. Automated enforcement

The following checks are available locally. `.github/workflows/rust.yml` also
runs `tools/check-rust.sh` on macOS for pushes, pull requests and manual runs:
format, locked check, strict debug/release all-feature Clippy and all-feature
production builds. The 0.11.0 cleanup removed all Rust automated tests. The full desktop script adds Python asset/package checks and
real-window/GPU checks; see [VERIFICATION.md](VERIFICATION.md).

| Rule | Enforced by | Command | Fails? |
|---|---|---|---|
| Catalog is valid, references resolve, ids unique, level references valid | `tools/assets/validate.py`; runtime catalog parser | `python3 tools/assets/validate.py` | yes (exit 1) |
| PNG exists, real PNG, non-zero, ≤1024 (sky: 2:1 POT, ≤2048×1024) | `tools/textures/build.py --check` | `python3 tools/textures/build.py --check` | yes (exit 1) |
| Over-preferred (>256) warning; non-POT warning | `tools/textures/build.py --check` | same | no (warnings only) |
| Environment surface seams | `tools/textures/seam_repair.py --check` via `tests/test_package.py` | `python3 -m unittest tests.test_package` | yes |
| Fixture faces: POT, unique sheet, ≤1024 | `tests/test_package.py` | `python3 -m unittest tests.test_package` | yes |
| NO DIVING sign: 1024², RGBA, transparent pixel | `tests/test_package.py` | same | yes |
| Prop GLB: container parses, one mesh, `TEXCOORD_0` present | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) for a missing or unparseable model; budgets, UV range and scale/origin are not evaluated here |
| Prop GLB decoded texture memory (per-texture and pack total) | `tools/props/build.py --check` | `python3 tools/props/build.py --check` | yes (exit 1) |
| Model textures: embedded PNG only, ≤1024, UV bounds | `src/engine/gltf.rs` parser (runtime) | compiler / game run | parser error / fallback box |

The Rust-only shipped-sheet, fallback identity, fixture orientation and prop art
budget assertions were removed with the Rust suite. Their asset contracts remain
mandatory; retained tooling covers the checks listed above, and production
parsers still enforce runtime format/security limits. A green build alone does
not establish compliance with an artwork contract.

Known enforcement gaps (checked here, not automated):

* no minimum source resolution for any class;
* pool surface squareness and opacity beyond retained tooling coverage;
* decal non-POT is only a tooling warning for the small sheets;
* `tile_metres` is not compared against the painted repeat period;
* surface and decal orientation, and the "pale albedo" convention, are not
  machine-checked;
* no dead-asset detection (unreferenced files, unused catalog entries);
* the `--preferred` warning does not fail a build;
* CI covers the shared Rust gate; a green CI run does not replace local asset
  tooling or executed native desktop validation on the target platform.

---

## 25. Uncertainties and known inconsistencies

Unresolved items below are **not** settled contracts. Do not build new assets on them
without checking the implementation.

1. **Resolved: fluorescent rotation preserves the fitted sheet.** Both 90° and
   270° placements use local +X → world +Z and local +Z → world −X, matching the
   existing ceiling-fixture axis-swap rule. Borders and UVs rotate together
   without mirroring, and the mandatory 2:1 artwork contract is preserved (§6.1).
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
6. **Platform icon integration remains separate.** The approved non-runtime
   source lives below `assets/shared/branding/` and is excluded from runtime
   payloads; bundle metadata does not configure a native platform icon resource.
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
11. **No minimum sizes and no per-class maximum below the ordinary 1024, and the
    pack budget is aggregate.** A 16×16 surface sheet passes the dimension
    tests; ordinary prop sheets are capped at 256×256, with reviewed cloth 1024 /
    wrap 512 exceptions, and the toolkit/Rust tests enforce both these
    authoring limits and the 128 MiB decoded pack budget, but a single pathological sheet below the ordinary 1024 edge cap
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
17. **Pack material parsing is dormant.** `src/engine/materials/pack.rs` still parses a
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
python3 -m unittest tests.test_package            # full asset/environments/level gate
```

Rust:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --locked --release --workspace --all-features
```

Regeneration paths (only when intentionally changing artwork):

```sh
python3 tools/textures/build.py [--only <id>] [--force --only <id>]   # surface/decal/fixture PNGs
python3 tools/props/build.py [--only <id>]                          # prop GLBs
python3 tools/props/animate_spooner_man.py [--check]                # the entity rig
```

Per §2, regeneration never replaces shipped artwork whose dimensions differ
from its painter's output unless `--force` is passed; a plain run is safe.

### October 10 environment refinement additions

Four geometry assets reuse their themes' unchanged fitted opaque 256² PNG atlases:
Beach `town_terrace_base` and `cove_foam`, Aero `portal_spandrel`, and Winter
`snow_roof_blanket`. They retain +Y up, +Z front, floor/base origin and UVs in 0..1.
The 48 m cove foam is one closed scalloped shell shared with the supported coast
profile; it contains no repeated internal end caps. The Winter blanket spans both
roof bays, with end overhangs and an eave lip derived from actual shingle stock.
The four models' embedded atlas pixels match their unchanged standalone PNGs;
the Winter exporter losslessly re-encodes the existing snow artwork. The
Winter blanket uses a continuous mirrored atlas unwrap at 3.4 m per U repeat and
2 m per V repeat, retaining its authored surface. No new raster class, alpha
behavior or texture resolution is introduced.

The Winter sky retains its 2048×1024 opaque RGB, equirectangular U-repeat/V-clamp
contract. `tools/textures/normalize_sky_seam.py` is an explicit offline correction
for the saved PNG: it joins only the left/right columns, feathering at most 16
columns per side. It preserves every interior pixel, dimensions, PNG mode, alpha
and metadata; it never joins the vertical poles. The shipped correction changes
at most 4/255 per channel. All four catalog sky sheets have exact U edge matches.
The normal runtime loads these committed PNGs through the existing sampler.


### October 14 completed model adoption

`assets/completed-models.json` records the 103 accepted v36 runtime exports for
107 identities, their exact SHA-256, measured bounds and reviewed geometry/image
budgets. Except for the documented Bepsi print trim below, GLBs retain the
delivered bytes. Catalog `authoring.tool: completed`
prevents the original primitive/Blender exporters from replacing them; final
editable masters remain in Downloads/Places-completed-models-2026-10-14.
The original Spooner-Man body-only refiner and party/Halloween source writers
remain historical tools. Current checks validate accepted export identity,
actual rig/clip boundaries, bounded Rust playback and native placement instead
of requiring the superseded head, joint order or authoring scene.

Larger sheets preserve their fitted authored frames: ordinary Shared 256;
reviewed entities 256/512, Arcade family atlases 256/512, party tableware 512×256,
cloth 1024², gift 512², cake 1024×256 and restroom 1024×512. All are embedded PNGs
with equivalent unique standalone PNG sources under assets/. No image is
resized, regenerated, repacked or given a new alpha contract during adoption.
Identical decoded RGBA images (dimensions included) share an Arc across models;
numeric fluorescence values remain exact. The 128 MiB unique decoded pack budget
is retained; per-GLB embedded copies are storage representation, not separate
resident images. Weak ownership releases unused family sheets on dependency
changes. Toolkit accounting conservatively deduplicates exact embedded PNG bytes.

Four accepted rectangular cloth exports contain a fitted U overscan reaching
1.0495164. Their hash-verified import profile permits only the upper 0.05 gutter,
preserving the complete authored UV layout and the existing CLAMP_TO_EDGE
sampling semantics. No tiling or repeat is introduced. Ordinary and unapproved
GLBs keep the 0..1±0.01 contract; replacing any accepted bytes fails identity
verification before this profile can apply. Round cloths retain ordinary UV bounds.

Five streamer catalog aliases use one untextured neutral `party_streamer.glb`.
Their `tint` is authored linear RGB in 0..1 and is applied to each static instance's
albedo vertices before packaging, independently of lighting and emission. All
five keep the same shared decoded mesh; no PNG or GLB is duplicated per colour.
The shared white runtime sheet handles untextured triangles. Developer previews retain the same per-object albedo tint through a bounded
GPU material uniform, without copying mesh or image payloads or altering alpha,
fluorescence or emission; balloon body colour retains its named analytic rig path.

Entity rigs preserve their delivered root pivots, including asymmetric bind
silhouettes. Their catalog X/Z envelopes conservatively cover both sides of the
root; measured min/max remain in the review record. The runtime still uses the
actual posed geometry for bounds. Two new 1.980×0.990722×0.605500 m benches are
ordinary non-emissive floor props with identical structure and swapped upholstery.
Reviewed triangle exceptions are bounded to the exact named exports: up to 2430
for cabinets, 1620 for benches, 1548 for neon, 1532 for pinball and 5688 for the
Spooner-Man rig. Ordinary 1500/3000 art budgets and 6000 engine ceiling remain.

Full receiver geometry for the denser completed Arcade models exceeds the former
six-page allowance with two switch circuits. The typed atlas record now admits
384 MiB plus 64 KiB container overhead, sufficient for eight 1024px pages across
three illumination groups. Sampling/gutters, the 512 MiB KTX2 payload guard, exact
shape/length checks and 1 GiB aggregate cap remain. This preserves actual Full
lighting instead of accepting the whole-world vertex fallback; native memory
and timing must be reported against the valid pre-adoption package.

At a nearly grazing receiver, filtering can retain positive neighbouring
surface energy while the aggregate directional moment nearly cancels. If its
f32 calibration fails the existing strict cosine-integral check, the compiler
stores that finite nonnegative integral isotropically for that texel. Ordinary
directional calibration and the validation tolerance remain unchanged; this
avoids huge opposing coefficients rather than amplifying rounding error.

Bepsi's v36 lower print and dispenser rim shared local Z=0.4320000112 across
15–17 mm bands. `tools/props/trim_bepsi_dispenser.py` trims only twelve print
vertices to the rim's outer opening plus 0.2 mm clearance, preserving the
original affine UV scale. Other primitives, embedded PNGs, normals, indices,
materials, bounds and collision remain unchanged. The review records both
original delivery and corrected runtime hashes. Reapply this hash-guarded
offline recipe to the original v36 export; completed ownership prevents an
older exporter from silently restoring the overlap. Downloads retains the
original editable master and export for source analysis.

Full transport receiver identity uses the existing numerical plane-normal
tolerance (eight f32 epsilon), including shallow folds. A neighbouring face
within the former 0.9999 dot allowance could steal a bowl-edge receiver and
clip its coverage against the wrong plane, creating dark triangular seams.
Solver revision 21 corrects the identity without changing model artwork,
normal transforms, illumination powers, brightness response or atlas density.

### October 15 Pool and Office adoption

The additional 23 Pool, nine Office and one snack-vending exports retain their
delivered GLB bytes. `assets/completed-models.json` binds each export's hash,
measured min/max bounds and reviewed texture edge. Mounted fixtures, the
wall-centered noticeboard and asymmetric fittings retain those measured local
origins. Their runtime check verifies both hash and bounds; ordinary unreviewed
assets retain floor-contact and centering requirements.

The embedded PNG sources have equivalent standalone files beside their models;
the four unchanged Office replacements retain their delivered 1024² masters.
No UV layout, alpha mode, image pixels or dimensional shelf/product geometry is
re-authored. Complete editable/reference deliveries remain under the ignored
`import/model-rework/` collection and in their original Library items.

The snack machine retains 16,240 triangles, 13 material groups, one shared 512²
atlas, five recessed shelves, 50 packets, 25 coils and actual thin BLEND glass.
Its exact accepted export alone declares `engine_triangle_limit: 16240`; the
cache checks its SHA before the bounded private importer profile applies.
Ordinary/public GLB imports still stop at 6,000 triangles. The source material
groups already consolidate wrapper shaders; metal remains non-emissive.
The model is 1.9 m tall with bottom-centered cabinet origin; its forward controls
are intentionally included in the larger authored collision envelope.

The revised drinks machine retains the exact R2 export: 2,869 triangles and
12 opaque material groups in the original 1 × 1.9 × .8 m envelope. Its fitted
`drinks_fascia.png` is 256 × 512 and `drinks_materials.png` is 256²; both have
standalone equivalents beside the GLB. Vent, cavity and label surfaces are
intentionally open, with finite unit normals and valid winding. Fascia slot 4
`Office_vending_mountain_art` retains its authored override; LCD slot 6
`DRINKS_07_LCD_EMISSION` retains delivered emission. Both follow the existing
shared vending circuit, together with its two unchanged exterior spill sources.
All metal and other material slots remain non-emissive.

Fixed props may author numeric `emission_overrides` while retaining the delivered
GLB and fitted PNGs. A shared `light` component controls only luminous slots and
their actual attached sources. Full selects prepared contributions for steady,
flickering and dark states; Static holds the authored initial state. This adds
no dynamic rebake. See the map guide §21 for the field and controller contract.

### Beam black mannequin preset (October 2026)

`core:mannequin_black` is an animatable prop preset of the currently installed
`entities/mannequin/model/mannequin.glb`, with linear albedo tint
`[0.04, 0.045, 0.055]`. It shares the original embedded/standalone PNG, UVs,
alpha, geometry, rig, normals and clips byte-for-byte; it creates no replacement
artwork and adds no emission. Catalog prop tints also multiply placed/runtime
character vertex albedo, so an animatable prop preset matches its static fallback.
The fitted texture still supplies grain and joint detail under actual light.
The approved corrected R2 shares this model path; the preset tint and all
existing placement/physics values remain unchanged.

### October 18 Home/shared, duck and Janitor integration

The 32 Home and five shared replacements retain exact decoded original bounds,
fitted PNG pixels and emission roles. Completed-export ownership now prevents
historical builders from restoring superseded geometry. The switch preserves
`switch → lever_pivot → lever`, translations and exact binary `toggle` samples;
node indices may differ without changing that named hierarchy.

The rebuilt duck preserves its 0.098 × 0.12 × 0.145 m occupied bounds and +Z
bill. Its opaque 128² PNG is `environments/pool/props/rubber_duck.png`.
`tools/props/adapt_rubber_duck.py` removes only the optional unsupported
`KHR_materials_specular` metadata from the hash-guarded delivery. Core PBR
roughness .58/.48/.48, metallic zero and BIN bytes remain unchanged. The reviewed
2,416 triangles retain the approved faceted silhouette; this is an asset-specific
art-budget review, below the ordinary 6,000 engine ceiling. Existing collision,
mass and physical buoyancy are unchanged. The supplied 51e672fc… checksum is the
raw GLB checksum; the original ZIP checksum is e57c3bb9….

`assets/janitor_closet/models/` and `textures/` preserve the new delivery's
structure. These are existing model/prop classes with opaque fitted, clamped
PNGs, metre scale, +Y up, +Z front and delivered base pivots. The shared
1024² atlas retains surface grain and small fitted labels; the caution print
remains 256×384 (2:3). Both have exact standalone PNGs. Medium/Low use existing
192/128 budgets. No new image class, shader, extension or runtime generator is
introduced. Slate-blue towels use the existing per-instance tint on one GLB.

Exact per-model min/max and texture edges are pinned in completed-models.json.
Four reviewed assembled assets exceed 1,500 art triangles: cart 1,616, rack
1,784, crate 2,508 and wringer bucket 2,578. Their open storage, thin stock,
perforations and wringer parts provide visible depth and retained source
editability; all remain below the 6,000 engine limit. Do not increase a global
import budget for them. Source previews and native validation are separate
evidence, described in the integration report. The current Spoonerman export
remains unchanged; its rejected replacement is excluded.

### Approved mannequin R2 (October 18)

The exact approved export is pinned in `completed-models.json`: 2,412 triangles,
37 joints and 19 clips. Its 1024-square opaque fitted PNG is stored beside the
entity's historical source sheets and embedded unchanged. Preserve all six
original clip names, the 17 original body rest transforms and 20 finger
descendants. Keep existing catalogue/placed envelopes and black-preset tint;
the asset is not resized to fit interaction props.

Zero-duration poses hold, declared loops repeat, and door/handling/chair
one-shots clamp at their distinct endpoints. The boundary checker respects
explicit clip loop metadata. Baked root displacement remains relative to the
actor placement; do not also transfer it into actor movement. Delivered event,
root and contact tracks remain reference data for future controllers. The asset
integration stage added no prop grasp, door synchronization or runtime IK;
existing glow sockets follow posed nodes. The later environment collection adds
an explicit socket attachment and atomic held-item handoff using the unchanged
approved `hand.R`, `carry_idle` and `carry_walk` data. This controller does not
resize the model or add runtime IK. See the handoff contract in `SCRIPTING.md`,
the integration report and editable
`projects/mannequin-r2-review/` for the opt-in native review.

### 0.12.0 menu use of project thumbnails

The existing 16:9 `thumbnails/<hash>.png` contract also supplies menu icons and
editor previews. Preserve the whole RGB/RGBA8 image and alpha; no square crop,
resize, new metadata or world-catalog entry is introduced. UI sampling is clamped
linear, independent of world texture quality. A selected level's image wins;
only the entry map inherits the project thumbnail, otherwise a code-native
framed P is the fallback. Decode uses the existing 4 MiB encoded/8 MiB bounded
thumbnail path (up to 1280×720), not the world texture edge limit. The renderer
caches at most eight visible images and performs no per-frame PNG decoding.


### Home folding closet and October 20 completed replacements

`home:closet_folding_door` is a new ordinary rigid animated prop with exact
final runtime GLB SHA-256
`8e713092feff04f385606ad2b9e69bfdf07638ad5d3e44981cbb72894227e9d9`.
It retains 792 triangles, 1,504 exported vertices, ten primitives, two opaque
materials and one fitted opaque 256² PNG beside the GLB under
`environments/home/props/`. Its four atlas quadrants are painted stock,
recessed paint, aged hardware and dark track. UVs remain fitted/clamped and
unchanged; 1024² masters and editable archives stay outside runtime distribution.
Closed bounds are 1.96×2.18×0.224 m, floor-centred, +Y up and +Z front.
The original `fold_open` LINEAR quaternion clip lasts 1.4 seconds and reverses
for closing. Preserve named pivot hierarchy, rear fold-hinge offsets and all
four independently folding leaves. Full-open measured clear width is 1.279497 m;
forward sweep is 0.435504 m. It requires clip-driven individual live collision,
not a solid prop envelope or a native single swinging slab. See the map guide.

The 144 supplied replacement exports retain exact delivered GLB/PNG bytes,
original canonical identities, measured bounds and semantic material roles.
Their accepted-export ledger protects them from historical procedural rebuilds.
The Outdoor grass card MASK exception and ghost-cat's intentional lower cloth
hem are reviewed open surfaces; compound contact seams and constant-cell UVs
require their specific evidence, not blanket rejection by a mesh heuristic.
Original references, source masters and the previously accepted cohort remain
unchanged. New fitted Beach atlases are separate `beach_nature_fitted.png` and
`beach_fitted.png`; they do not replace an unrelated shared atlas.
Native acceptance and installed deduplicated memory accounting belong in the
single 0.13.0 release report. No global art, draw, texture or importer budget is
raised by this adoption.

## Shared mannequin appearance atlases

The accepted R2 mannequin supports optional catalog PNG albedo sheets through
`character_texture`, on placements and spawn templates. These are fitted opaque
1024x1024 sheets with the exact existing GLB UV layout and orientation. Do not
repack, rotate, change silhouettes, or duplicate GLBs/rigs/animation exports.
Man, Woman, non-graphic Zombie, Robot and lab-coat Scientist sheets live under
`entities/mannequin/textures/`. The approved GIMP replacements preserve exact
opaque RGBA8 pixels and their fitted layout; layered sources remain in Library.
The original sheet and Black tint are preserved. Albedo replacement affects the
first skin submesh only; recessed joint grooves retain the original material.
Source prompts are retained in `assets/references/entities/mannequin-skins/`.

## Celestial source discs

Opt-in Sun/Moon visuals use separate file-backed RGBA PNGs at
`assets/environments/outdoor/textures/celestial/{sun,moon}.png`, catalog IDs
`core:celestial_sun` and `core:celestial_moon`. Both are **1:1**, preferred
512x512, with the centered circular body fitting inside a transparent margin.
Alpha is real coverage (including a restrained solar halo), not a painted
background. Image top stays celestial north; no tiling, U/V clamp. The renderer
projects each fitted square at infinity around the negative normalized direction
of its owning Sun/Moon illuminator, and composites straight-alpha linear color
before world geometry. The authored visual angular diameter controls the fitted
square, independently of the light's shadow softness. Pixels and alpha are loaded
from the committed files; shaders do not synthesize a sun, moon or halo. These
are not 2:1 sky panoramas. Existing panoramas retain their original contract.

## Runtime audio assets

Shipped sound clips are real, committed mono PCM16 WAV files under
`assets/audio/<cue>/`, at 44,100 Hz, no longer than 30 seconds. Catalog entries
declare gain, loop and positional mode; bird/water beds retain their
non-positional defaults. HVAC is reserved for a single player-owned spatial voice near fixed props tagged `hvac-duct`; its
catalog default is overridden by that owner. A deliberate positional override
or a positional cue is required for a point emitter. Runtime preloads within
64 MiB decoded and 48 voices; no sample synthesis occurs at launch or map load.
`authoring.json`, `provenance.json` and `licenses/SOURCES.txt` retain recipes,
hashes and rights. `tools/audio/build.py --check` audits this closure offline.
The original fluorescent/pool/empty-room clips are authored by
`tools/audio/original.py` under the repository MIT license; the runtime never
runs that authoring tool. They have circular seams and rounded, sparse envelopes.


### 0.13.1 utility and HVAC kit

The 25 cloud-authored institutional GLBs retain their original embedded and
standalone 256-square PNG, plus packed Blender sources and layered GIMP master
under `tools/props/backrooms_phase6/`. Six HVAC pieces expose measured rectangular
connector faces for editor joins and the automatic spatial duct audio owner.
Five operational light IDs reuse the original fixture models with measured top
and luminous-plane offsets; only their diffuser/tube material emits. Passive
models, static doors, hollow stock, collision limits and source/hash conventions
are documented in [the kit integration guide](../tools/props/backrooms_phase6/INTEGRATION.md).
The unique new atlas costs 0.25 MiB decoded RGBA; summing its 25 embedded copies
costs 6.25 MiB. The reviewed 128 MiB pack ceiling preserves original visual quality.
