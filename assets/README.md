# Places asset architecture

`assets/catalog.json` is the authoritative registry of every logical asset the
game and its tooling know about. Levels reference assets by **logical id**
(`core:desk`, `spooner-man`, `core:carpet_beige_01`); the catalog maps that id to
everything else: its class, its optional environment theme, its type and its
canonical resource.

Nothing outside the catalog cares where a file lives. Moving a model or a
texture PNG between directories never requires editing a level.

```
logical id            core:desk / spooner-man / core:carpet_beige_01   (stable; levels store this)
    ↓
catalog metadata      assets/catalog.json                             (class, theme, type, resource)
    ↓
canonical resource    assets/<path>                                   (one physical file per asset)
    ↓
runtime cache         PropAssets / TextureCache / MaterialTable       (decoded once, reused)
```

Surface materials add one more link to that chain:

```
material id           core:carpet_beige_01        (levels store this)
    ↓                 catalog `texture` reference
texture id            core:tex_carpet_beige_01
    ↓                 catalog `model` path, relative to assets/
external PNG          environment/office/textures/floors/carpet_beige_01.png
    ↓                 decoded once per session, uploaded once per level
renderer material     texture + tile_metres + tint
```

## Quick start

```sh
python3 tools/assets/validate.py             # catalog, resources, shipped levels
python3 tools/textures/build.py --check      # surface PNGs exist, parse and fit their budget
python3 tools/props/build.py --check         # prop models exist and fit their budgets
cargo test
cd level-editor && npm test
```

## The catalog

```json
{
  "format_version": 2,
  "themes": [
    { "id": "office", "display_name": "Office", "description": "..." },
    { "id": "pool",   "display_name": "Pool",   "description": "..." }
  ],
  "assets": [
    {
      "id": "core:desk",
      "display_name": "Desk",
      "asset_class": "environment",
      "theme": "office",
      "asset_type": "prop",
      "source": "file",
      "model": "environment/office/props/models/desk.glb",
      "size": [1.6, 0.75, 0.7],
      "color": "#5f5142",
      "category": "Furniture",
      "solid": true
    },
    {
      "id": "core:tex_wallpaper_yellow_01",
      "display_name": "Yellow Wallpaper Texture",
      "asset_class": "environment",
      "theme": "office",
      "asset_type": "texture",
      "source": "file",
      "model": "environment/office/textures/walls/wallpaper_yellow_01.png"
    },
    {
      "id": "core:wallpaper_yellow_01",
      "display_name": "Yellow Wallpaper",
      "asset_class": "environment",
      "theme": "office",
      "asset_type": "material",
      "source": "definition",
      "surface": "wall",
      "texture": "core:tex_wallpaper_yellow_01",
      "tile_metres": 2.0,
      "tint": [0.85, 0.80, 0.42]
    }
  ]
}
```

Only `id`, `asset_class` and `asset_type` are required. Missing optional fields
inherit neutral fallbacks, and unknown future fields are ignored by this build.
The loader also accepts the legacy `props` array from the old flat registry, so
older tooling keeps parsing.

### Identity (`id`)

The stable name a level stores. Ids are non-empty names such as `core:chair`,
`office:monitor` or `spooner-man`; they never contain whitespace or path
separators, so a level can never smuggle a filesystem path in through an id.
Duplicate ids are a catalog error: the loader rejects the document instead of
letting the last entry win.

### Class (`asset_class`)

Broad semantic classification. `environment` and `entity` are the two classes
the game ships; `core` is the home for engine-level shared resources and
`diagnostic` for development content. Classes are validated identifiers, so a
future class parses and resolves without an engine change; tooling reports
unknown classes so typos (`enviroment`) cannot slip through.

### Theme (`theme`)

An organizational environment collection, absent for generic/shared content.
The initial built-in themes are **`office`** and **`pool`**. Themes are data:
add a `themes` record and future `hotel`/`school`/`residential` assets resolve
without touching Rust.

**Themes organize, they never restrict.** There is no code path that rejects an
asset because a room has a different theme; the runtime deliberately exposes no
theme-filtering query. Place Office assets in a Pool level, mix Office and Pool
in one room, or use an entity anywhere — placement is by id, nothing else.
Rooms have no mandatory theme field.

### Type (`asset_type`)

What the resource is: `prop`, `material`, `texture`, `light`, `decal` or
`entity`. Type is independent of class and theme. `prop` and `entity` assets are
*placeable* (they go through the ordinary model pipeline); materials, textures,
lights and decals are referenced by id.

### Source and resource (`source`, `model`, `texture`)

* `"source": "file"` assets name a `model` path **relative to this directory**.
  The runtime joins it onto the resolved asset root; one asset has exactly one
  canonical file. Props, entities and **textures** use this shape.
* `"source": "definition"` assets are data definitions composed from other
  catalog assets. A `material` names the logical `texture` it draws with, plus
  any render parameters (`tile_metres`, `tint`); it has no file of its own and
  must not declare `model`.
* `"source": "generated"` assets (the decal atlas patterns, the built-in
  light fixture) have no file. The renderer generates them in code; the catalog
  records their identity and classification so themes and future tools can see
  them.

## Surface materials and textures

A **material** is the game-facing surface definition a level names: it carries
the world-space tiling period and the static tint, and it points at a
**texture** asset that owns the PNG. Splitting the two is what keeps identity
stable: a level always names the material, so the artwork behind it can be
replaced or the file moved without touching a level.

* `"texture"` — the logical id of a `texture` asset. **Required** for every
  material; a material without one is a catalog error.
* `"tile_metres"` — world metres covered by one repeat of the texture, in both
  directions (default `2.0`, allowed `0.05`–`64`). A wall passes its length and
  height, a floor/ceiling its `x`/`z`, and both divide by this value at the
  material's tiling, so a two-metre sheet is still a two-metre sheet when the
  image becomes external.
* `"tint"` — three channels in `0..1` multiplied into the sampled texture
  (default `[1, 1, 1]`). The built-in office surfaces use it to keep their
  historical look: wallpaper `[0.85, 0.80, 0.42]`, ceiling
  `[0.72, 0.72, 0.70]`.
* `"surface"` — `wall`, `floor` or `ceiling`, documentation/validation only.
  The geometry being emitted decides which surface family a material draws on;
  a level may use any material on any surface.

Texture assets are just files:

* `"model"` must be a `.png` path relative to `assets/`; other extensions are a
  catalog error and a missing/undecodable file is a level-load error with the
  material id in the message.
* PNG dimensions are read from the file, not the catalog. The runtime accepts
  any non-zero size up to 1024×1024 and normalises RGB, RGBA, grayscale,
  grayscale+alpha, palette and 16-bit images to RGBA8. `tools/textures/build.py
  --check` enforces that budget and warns above 256×256 (the PocketCHIP/Mali-400
  budget) and for non-power-of-two sizes (portability: OpenGL ES 2.0 does not
  guarantee NPOT + repeat + mipmaps; the desktop GL path used on macOS loads
  NPOT fine, and `core:tex_diagnostic_alt_01` is deliberately 96×64 to prove
  it).
* Surface textures are uploaded with `REPEAT` wrapping and mipmaps. Alpha is
  decoded and preserved, but base surfaces are opaque and blending is off (only
  the decal pass alpha-tests), so keep surface PNGs opaque unless you are
  deliberately authoring a decal-style asset.

### Decal sheets

Decals are small local surface markings (signs, floor arrows, warning marks)
placed on an existing surface. A level names them the same way it names a
material — by logical id in its `decals` array — and the catalog decides where
the pixels come from:

* `"source": "generated"` decal assets are the built-in patterns the renderer
  draws into one shared atlas (`core:decal_test_01`, `core:decal_arrow_01`,
  `core:decal_stripes_01`). They are architecture-test artwork.
* `"source": "file"` decal assets are **external PNG sheets**, exactly like a
  surface texture: the entry names the `.png` file under `assets/`, the runtime
  decodes it once per session and uploads it as its own decal sheet, and a
  creator replaces the PNG and restarts. `core:decal_no_diving_01` (the Pool
  safety sign) is the built-in example.

Decal sheets need **power-of-two** dimensions (they are sampled with mipmaps
and `REPEAT` wrapping) and an alpha cut-out: the decal pass discards every texel
below alpha 0.5, so the background is alpha 0 and the artwork is the silhouette
plus its plate. The sheet is drawn in the decal pass with a fixed polygon
offset, so it wins the coincident-depth test against the surface it lies on and
still receives the baked lighting of the room it is in. An authoring workflow
lives in `tools/textures/README.md`; a missing or undecodable sheet draws the
same magenta/black diagnostic a broken surface texture does, with the decal id
in the console message.

### Light fixtures

A placed light names a `fixture` id from the catalog's `asset_type: "light"`
entries, and that id selects both the fixture appearance and the luminous
footprint the bake treats as a light source. The built-in families are:

| fixture id | appearance | mounting |
| --- | --- | --- |
| `core:fluorescent_panel_01` | recessed 1.2 x 0.6 m twin-tube office panel | ceiling (height derived from the room) |
| `core:pool_light_round` | round recessed downlight, 0.44 m | ceiling (height derived from the room) |
| `core:pool_light_wall` | shallow wall luminaire | wall: needs `"mount": "wall"` and a world `"y"` |

Fixture appearance and emitted light are separate: `color` and `brightness` are
authored per placed light and drive both the visible panel and the illumination
the bake applies. An unknown fixture id keeps loading and draws as the office
panel, because the catalog/renderer consistency test reports the mismatch
instead of the renderer failing at load. Adding a new appearance is a code
change (see `src/lighting.rs::fixture_profile` and the fixture batch in
`src/render.rs`), because a fixture is generated geometry, not artwork.

### Level packs and custom textures
A `.zip` level pack can ship its own surface art without touching the catalog.
Put the PNGs under `textures/` next to `level.json` and map material ids in
`materials.json`:

```json
{
  "materials": {
    "pack:wall": { "texture": "textures/my_wall.png", "tile_metres": 3.0,
                   "tint": [1.0, 1.0, 1.0] },
    "pack:carpet": { "texture": "core:tex_carpet_beige_01" }
  }
}
```

* The string form (`"pack:wall": "textures/my_wall.png"`) still parses, so old
  packs keep working.
* A `pack:` material with no mapping falls back to `textures/<name>.png` inside
  the pack, as before.
* A mapping may name a logical catalog texture id (anything with a `:`) to
  reuse shipped artwork; the pack's own `tile_metres`/`tint` still apply.
* Pack textures are decoded with the pack's namespace in the cache key, so two
  packs shipping `textures/wall.png` never share an image; the GPU copies are
  freed when the next level loads.
* A mapping that names a file the pack does not contain is a named error in
  the console, not a silent substitution.

### Adding a new surface material

No Rust change is required. From the repository root:

1. Add the PNG below `assets/`, e.g.
   `assets/environment/pool/textures/floors/pool_tile_blue_01.png`.
2. Register the **texture** in `assets/catalog.json`:

   ```json
   { "id": "pool:tex_tile_blue_01", "display_name": "Blue Pool Tile Texture",
     "asset_class": "environment", "theme": "pool", "asset_type": "texture",
     "source": "file", "model": "environment/pool/textures/floors/pool_tile_blue_01.png" }
   ```

3. Register the **material** that names it:

   ```json
   { "id": "pool:tile_blue_01", "display_name": "Blue Pool Tile",
     "asset_class": "environment", "theme": "pool", "asset_type": "material",
     "source": "definition", "surface": "floor",
     "texture": "pool:tex_tile_blue_01", "tile_metres": 2.0 }
   ```

4. Reference the material from a level (`defaults.floor`, a room, a wall face,
   a floor patch or a region).
5. Run `python3 tools/assets/validate.py` and `python3 tools/textures/build.py
   --check`, then launch the game. `python3 tools/assets/validate.py` reports
   duplicate ids, dangling texture references, missing PNGs and malformed
   metadata with the offending id in the message.

The catalog's `themes` list is data too: a `pool` theme already exists, and any
new theme id resolves without an engine change.

### Replacing a texture

Same material id, new pixels:

1. Open the PNG at the path in the texture entry (for the built-ins,
   `assets/environment/office/textures/...`).
2. Edit or replace it, keeping the file name. Keep dimensions sane; changing
   size is allowed, changing nothing else in the catalog is required.
3. Restart the game. There is no live hot reload, and **no recompilation**:
   image pixels are read at level load and decoded once per session.

This is the same for creators: `tools/textures/build.py` regenerates the seed
artwork deterministically, but hand-painted PNGs are just as valid.

### Where the artwork lives

```
assets/
  catalog.json                     authoritative registry
  prop_proxies.json                derived editor previews (never hand-edited)
  README.md                        this document
  levels/                          shipped levels (assets referenced by id)
  environment/
    office/
      props/models/*.glb           office furniture
      textures/walls/*.png         wallpaper (maintained, stained)
      textures/floors/*.png        carpet (maintained, damp)
      textures/ceilings/*.png      panel ceiling (maintained, stained)
    pool/
      props/models/*.glb           patio table and chair, curtains, ladder, guardrails
      textures/walls/*.png         wall tile
      textures/floors/*.png        deck and basin tile
      textures/ceilings/*.png      sterile ceiling
      decals/no_diving_01.png      the final safety sign (RGBA cut-out)
  core/props/models/*.glb          shared/generic props
  entities/spooner-man/model/spooner-man.glb
  diagnostic/
    textures/*.png                 architecture-test artwork (orientation, alpha, NPOT)
```

Directory neatness is the lowest priority behind compatibility: the catalog is
what the runtime reads, so files may move freely as long as the catalog follows.

## Prop and entity conventions

* 1 model unit = 1 metre; the engine is right-handed, **Y up**, floors at `y = 0`.
* The origin sits on the floor-contact point, horizontally centred under the
  object's true bounding box.
* **+Z is the front**: fridge doors, the TV screen, the vending machine panel,
  the couch seat all face `+Z` at `rotation_degrees = 0`.
* A model's bounding box must match the catalog `size` within
  `max(2 cm, 6 % of the axis)`; `tools/props/build.py` fails otherwise.
* `size` is the catalog's rendering/editor box. A level's **collision** box is
  the prop's own `size` (plus its `scale`) when authored, and the neutral
  `PROP_FALLBACK_SIZE` (0.6 x 0.9 x 0.6 m) when it is not — the catalog size is
  never collision-tested, and props are never tested against their render mesh.
  A solid prop that should block like its picture therefore authors `size` in
  the level, with the footprint's x/z swapped for a 90/270 degree rotation
  (collision boxes are axis-aligned). Intentional clipping (props sunk into
  floors, overlapping walls or objects) is allowed and never corrected.
* Levels place props with `x`, `y` (vertical offset, may be negative), `z`,
  `rotation_degrees` (Y), `scale` and an optional `size` override.

## Budgets (PocketCHIP / Mali-400, 480×272 display)

| budget     | value                                                      |
| ---------- | ---------------------------------------------------------- |
| triangles  | 50–500 preferred, ≤800 acceptable, 1500 hard ceiling; props above 800 are allowlisted in `src/props.rs` with a written reason (`spooner-man`, a creature, needs 928) |
| prop texture | 64×64 or 128×128 preferred, 256×256 hard ceiling         |
| surface texture | 128×128 preferred, 256×256 soft warning, 1024×1024 hard load ceiling |
| materials  | exactly one diffuse texture per prop                       |
| draw calls | one per distinct model per level (instances are baked)     |

Baked vertex colours carry the per-face shading and contact darkening (the same
`PROP_FACE_SHADES` the old placeholder boxes used); the shader stays
`texture2D(u_texture, v_uv) * v_color`. No normal maps, no PBR extensions, no
alpha, no animation, no skinning, no morph targets.

Surface materials multiply the same way: the sampled texture is scaled by the
material's `tint` and then by the baked lighting exactly like the old
code-generated sheets, so RGB lighting keeps working on external artwork.

## PNG conventions for surface textures

* **Opaque** RGBA or RGB; 8-bit.
* **Tileable** in both directions: the right edge must join the left, the top
  the bottom. `tools/textures/build.py --check` does not verify tileability
  (that is an art check), but the seed generator wraps all of its noise.
* **Wall orientation**: the image's top row is at the top of the wall and its
  left edge is on the viewer's left from the side the face looks into, so
  signs/borders read correctly on both sides of a partition. A tile is
  `tile_metres` tall; the phase is anchored to the wall top.
* **Floor/ceiling orientation**: image `x` maps to world `+X` and image `y`
  maps to world `+Z`, so an image authored map-style reads with north (`-Z`) up.
* **Colour space**: no gamma handling. The historical sheets were authored pale
  because the material tint and the baked lighting multiply into them; author
  with that in mind or set the tint for your material.

## GLB profile (props and entities)

One scene, one node, one mesh, one primitive, one material, one embedded PNG
image. Attributes: `POSITION` (float32 vec3), `TEXCOORD_0` (float32 vec2),
`COLOR_0` (normalised uint8 vec4), 16-bit indices, `mode: 4` (triangles).
Self-contained: no external `.bin`, no external textures, no extensions.
Anything else is rejected by `src/props.rs` with an actionable message.

Props keep their textures embedded in the GLB: only level surfaces (and future
decals/fixtures) load external PNGs.

## Adding a future asset

1. Add the catalog entry (id, class, theme when it belongs to one, type, size,
   colour, category, solid, `model` — or `texture` for a material).
2. For a generated asset (light fixture, decal sheet), implement or extend the
   renderer's generator and add the id there too; the catalog/renderer
   consistency test will catch a mismatch.
3. For a textured surface, add the PNG and the two catalog entries above; no
   Rust code is involved.
4. For a modeled asset, add a build function to `tools/props/parts/*.py` and
   register it in that module's `PROPS` dict (see `parts/utility.py` for the
   commented exemplar).
5. `python3 tools/props/build.py --only core:your_prop` — this enforces the
   scale/origin/UV/budget rules and writes the GLB at its catalog path.
6. `python3 tools/props/preview.py --only core:your_prop` and look at
   `target/prop-previews/your_prop.png` before trusting it.
7. `python3 tools/assets/validate.py`, `python3 tools/textures/build.py
   --check`, `cargo test` and `cd level-editor && npm test`.

Nothing here is required at runtime: the game loads ordinary packaged GLBs and
PNGs.

## Errors you may see

| message | meaning | fix |
| --- | --- | --- |
| `{id}: material texture `{tex}` is not declared in the asset catalog` | a material points at a texture id that does not exist | add the texture entry or correct the id || `{id}: a material must declare the logical `texture` it draws with` | a material entry has no `texture` | add one (or make it a `generated` asset of another type) |
| `{id}: a texture asset must name a `.png` file, found `...`` | texture `model` is not a PNG | convert the file and update the path |
| `[materials] {level}: material `{id}` texture `{tex}`: cannot read ...` | the PNG is missing at load time | restore the file; the surface shows the magenta/black diagnostic pattern meanwhile |
| `[materials] {level}: unknown material `{id}`; add it to the asset catalog ...` | a level names a material the catalog does not declare | add it (or fix the typo); the surface shows the diagnostic pattern |
| `[materials] {level}: material `{id}` texture `{tex}`: `...png`: PNG decode error ...` | the PNG is truncated or corrupt | re-save it; see the loading tests for the accepted encodings |
| `[decals] decal `{id}`: {problem}` | an external decal sheet's catalog entry or PNG is broken | fix the entry/path; the decal draws the diagnostic sheet meanwhile |
| `[textures] ...` (from `tools/textures/build.py --check`) | file missing, corrupt, oversized or non-PNG | regenerate with `python3 tools/textures/build.py` |

Validation fails loudly in tooling and degrades visibly in game: a broken
texture is never hidden behind unrelated artwork.

## Development fixtures and checks

Eight demo levels exercise the pack (none of them touch `level1`):

* `levels/asset_demo.json` — **the walkable demo map**, discovered in the game's
  custom-level folder and shown in the level select menu. Four rooms around a
  corridor place every generic and Office placeable asset (the core props plus
  `spooner-man`), and it exercises the whole level format: doorways, a wide
  passage, windows, a vent, twelve ceiling lights, several props standing on
  other props, and the worn material set (stained wallpaper, damp carpet,
  stained ceiling) that Level 1 does not use. The Pool family lives in the Pool
  showcase instead, so a residential demo never has to hold a pool ladder.
* `levels/asset_maintained.json` — the same building on the maintained material
  set (yellow wallpaper, beige carpet, panel ceiling). A level carries one wall,
  one floor and one ceiling material, so the two demos are how the maintained
  and water-damaged sets are compared in game.
  Regenerate both with `python3 tools/levels/build_demo_levels.py`.
* `assets/levels/prop_showcase.json` — every generic and Office placeable asset
  placed once, arranged as a domestic room plus a utility room, including one
  crate deliberately sunk into the floor and a box overlapping it.
* `assets/levels/prop_stress.json` — ~150 repeated placements across nine
  models, used to prove that instances share one decoded model, one texture and
  one draw call per model.
* `assets/levels/vertical_diagnostic.json` — the vertical-geometry level:
  an elevated room reached by a region staircase, a walkable recess and a
  blocked deep recess, a gable room with eave/ridge fixtures, RGB-lit corners and
  decals. Run it with `LIMINAL_LEVEL=vertical_diagnostic`.
* `assets/levels/texture_diagnostic.json` — the Goal 4.5 external-texture level:
  the diagnostic wall/floor/ceiling materials, the 96×64 NPOT texture, an RGBA
  alpha sheet, a coalesced material-run overlay, decals on external surfaces,
  a gable ceiling, a walkable recess, a region staircase into an elevated room
  and warm/blue/white fixtures. Run it with
  `LIMINAL_LEVEL=texture_diagnostic`; see
  `tools/bench/notes/texture-material-validation.md` for the capture spots.
* `assets/levels/office_showcase.json` — the Goal 5 Office showcase: a small,
  mostly empty institutional suite on the final wallpaper/carpet/ceiling, with
  warm fluorescent fixtures, sparse desks and chairs, one room on the damaged
  material set and floor decals. Run it with `LIMINAL_LEVEL=office_showcase`.
* `assets/levels/pool_showcase.json` — the Goal 5 Pool showcase: the clean
  sterile Pool family, a real recessed empty basin (`floor_regions`), the
  walk-in step, the ladder standing on the basin floor, the patio table and
  chair, modular curtains and guardrails (with collision), both Pool light
  fixtures and the final external `NO DIVING` sign. Run it with
  `LIMINAL_LEVEL=pool_showcase`.

Checks to run before shipping an asset change:

```sh
python3 tools/assets/validate.py             # catalog, resources and shipped levels
python3 tools/textures/build.py --check      # surface PNGs and their budgets
python3 tools/props/build.py --check         # files exist, parse and fit the budgets
python3 tools/props/build.py --thumbs        # refresh the editor's prop thumbnails
cargo test                                   # catalog, scale, origin, UV and batching tests
cd level-editor && npm test                  # editor parses the proxies and draws real geometry
```

Useful developer-only run flags (they never affect normal play):

* `LIMINAL_LEVEL=prop_showcase` — boot straight into a level (handy on the
  PocketCHIP, where the menu is awkward over SSH).
* `LIMINAL_CAPTURE=frame.png` — render one frame and write it out, then exit;
  this is how prop rendering is inspected on hardware without a screenshot tool.
* `LIMINAL_SPAWN=x,z,yaw_degrees` (or `x,y,z,yaw`) — stand at a specific spot,
  e.g. in front of a prop that needs a close look.
* `LIMINAL_STATE_LOG=file.csv` — append `frame,x,y,z,yaw,pitch` every few frames
  so movement and control checks can assert real results from a running build.
