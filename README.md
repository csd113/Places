# Places

### an experience

A slow first-person exploration game. You walk through quiet, over-lit
institutional interiors — an office that keeps going, a swimming pool that is
closed and empty — and the building stops being finished around you. There is
nothing to collect, fight or solve.

Places is a desktop game written in Rust: `sdl2` for the window and input,
OpenGL through `glow`, and a baked-lighting renderer with no dynamic shadow
maps. Every fixture bakes into a lightmap atlas for static geometry, props
occlude the bake, and emission, transparency, selective reflections and a
restrained post-processing stage are all authored as content. Its content is
data: levels are JSON, surfaces are PNGs, props are GLBs, and a catalog maps
stable logical ids onto all of it.

![An office interior](docs/screenshots/01-office.png)

## Overview

The engine is deliberately small and the content is deliberately editable:

* **Rooms, walls and openings** are authored as rectangles in a JSON level.
  Walls can be cut with doors, windows, passages and vents.
* **Baked RGB lighting, now as real lightmaps.** Every fixture bakes a room
  baseline plus a local pool, exactly as before, but the result is stored per
  *texel* in a lightmap atlas for static floors, ceilings, walls and skirts — so
  a fixture reads as a pool of light with a soft edge instead of a plateau.
  Coloured fixtures tint both the visible panel and the illumination. The old
  per-vertex bake remains as an exact fallback.
* **Props occlude the light.** A placed prop's own geometry joins the bake, so
  the floor under a machine darkens, a fridge blocks the pool behind it and
  furniture grounds itself against the wall it stands on — with nothing to
  author.
* **A separate dynamic-object path.** Objects whose transform changes every
  frame render outside the static batches and the bake: upload the model once,
  pass a transform per object, probe the baked light at its position. Moving one
  never rebuilds geometry, batching or lightmaps.
* **Partitions split the baseline.** When an opaque internal wall divides a
  room's footprint, each side gets its own baseline from the fixtures it can
  reach; a doorway still blends a bounded amount through its aperture, and a
  wall that stops short of the ceiling is not a partition. An open room bakes
  exactly as it always did.
* **Walls are lighting boundaries.** A fixture's light only reaches what its
  panel can see: an opaque wall blocks the pool behind it, and a doorway,
  window, passage or vent transmits light through exactly the hole it cuts.
* **Floors and ceilings are boundaries too.** Stacked rooms do not light each
  other through a solid slab, in colour or brightness, while a raised platform,
  a lowered basin and an intentional vertical opening stay open. A ceiling
  fixture on a chosen storey authors a world `y`.
* **Vertical geometry.** A room has its own floor elevation, clear height and
  ceiling profile (flat or gable); `floor_regions` recess or raise rectangular
  parts of a room, which is how the empty pool basin and the region staircases
  are built.
* **External artwork.** Surface materials, decal sheets, light fixture faces and
  (optionally) level pack textures are ordinary PNGs under `assets/`. Replace
  the file, restart, see the new pixels — no Rust change and no recompilation.
* **A real catalog.** Levels never store a file path. They name a logical id
  (`core:desk`, `core:pool_tile_deck_01`, `spooner-man`) and `assets/catalog.json`
  resolves it to a file, a material definition or a generated resource.
* **Restrained post-processing.** The scene is drawn into an offscreen target and
  resolved into the display image: bloom that follows *emission* rather than
  brightness, a tone shoulder that leaves everything below 0.75 untouched,
  distance fog and a barely-there grade. The HUD draws after it, so the UI is
  never touched by any of it, and `Low` skips the whole stage.
* **Selective reflections.** A material can ask for a static **probe** (a small
  cubemap baked once per level load, one texture read a fragment) or a **planar
  mirror** (a real second view of the level through the surface's own plane, at
  most one plane per frame). Both are weighted by the sheen the material already
  authors, so a rough or dull surface suppresses its reflection instead of
  mirroring, and `shine` (`0.0` matte .. `1.0` glossy) is an authorable material
  property with an optional per-surface override in a level.
* **Animated emissions.** A level can make a material's emission `pulse` or
  `flicker`, deterministically and within a bounded depth — a backlit sign
  breathing, a tube on a failing ballast.

## Screenshots

| | |
| --- | --- |
| ![Office](docs/screenshots/01-office.png) Office: warm fluorescent panels over carpet and printed wallpaper, with the pool windows on the right. | ![Office window](docs/screenshots/02-office-window.png) The office looks one storey down into the pool through a window aperture. |
| ![Stair transition](docs/screenshots/03-stair-transition.png) The stair hall turns red; the pool is visible through both a passage and a window beside it. | ![Pool](docs/screenshots/04-pool.png) The empty pool: recessed basin, ladder, guardrails, patio set, NO DIVING sign, cool light. |
| ![Final doorway](docs/screenshots/05-final-doorway.png) The last doorway frames the unmade world. | ![Unmade world](docs/screenshots/06-unmade-world.png) Beyond it: a floor, a ceiling, a few fixtures, and nothing. |

## The demo

`assets/levels/places_demo.json` is the official showcase. It is one continuous
route through everything the project currently does:

```text
office reception  →  workroom  →  doorways and windows
      →  red stair hall (1.5 m down)  →  pool hall (recessed basin)
      →  two steps up  →  quiet corridor  →  final doorway  →  the unmade world
```

Walk it from the main menu, or boot straight into it:

```sh
cargo run                                   # then: Level Select → Places Demo
LIMINAL_LEVEL=places_demo cargo run         # straight into the demo
LIMINAL_LEVEL=places_demo ./Places/places    # from a packaged build
```

Route, if you want it: from the spawn, walk forward through the doorway into the
workroom, keep straight through the second doorway and down the stairs, follow
the passage into the pool hall, cross the deck to the far side, and take the two
steps up into the dim corridor. The doorway at its end is the last one.

## Controls

Menus use `W`/`S` to move through items, `ENTER` to activate and `ESC` to go
back; a Settings value uses `A`/`D` (or `ENTER`) to change it.

Settings is organized into three sections — **Graphics**, **Display** and
**Controls** — reached from either the main menu or the pause menu, so the game
can be reconfigured without leaving a level.

Gameplay defaults (all eight movement bindings can be changed in Settings →
Controls; `Restore Defaults` puts them back):

| Action | Key |
| --- | --- |
| Walk forward | `W` |
| Walk backward | `S` |
| Strafe left | `A` |
| Strafe right | `D` |
| Look up | `UP` |
| Look down | `DOWN` |
| Look left | `LEFT` |
| Look right | `RIGHT` |
| Pause menu | `ESC` (fixed) |
| Performance overlay | `-` (fixed, hidden by default) |

Graphics and display preferences, custom bindings, look speed, walk speed,
field of view and look inversion are saved to `settings.json` in the package
root and kept across launches. The defaults are a 1920×1080 windowed desktop
window; a fresh installation writes that configuration on first run. Changes
apply immediately: Graphics Quality (`Full` / `Low`), Bloom, Reflections,
Lightmaps, VSync, Texture Filtering, Window Mode, Resolution, sensitivity, look
inversion and bindings all take effect without restarting or reloading the
level. See [Rendering notes](#rendering-notes) for what each option changes.


## Build and run

### Desktop prerequisites

macOS is the development platform. You need SDL2 and `pkg-config`:

```sh
brew install sdl2 pkg-config
cargo run
```

`cargo run` resolves `assets/`, `levels/` and `settings.json` from the
repository root, and also from the executable's own directory, so running the
game from a subdirectory works too. A compiled build locates its payload from
its own path and does not depend on the working directory at all.

A first launch is self-initializing: the game creates the drop-in `levels/` and
`import/` directories and writes a default `settings.json` next to its payload
(or below `$LIMINAL_STATE_ROOT` when that is set), so nothing has to be prepared
by hand. Places Demo is always available, even if no asset tree and no level
files exist at all. Normal startup prints nothing; `LIMINAL_VERBOSE=1` turns the
developer telemetry (package/asset/level/lighting/build lines) back on for a
run. Genuine problems — a missing asset root, a skipped level file, an
unresolved material — are always reported, once each.

Validate the project:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 tools/assets/validate.py          # catalog, resources, shipped + fixture levels
python3 tools/textures/build.py --check   # surface, decal and fixture PNGs and their budgets
python3 tools/props/build.py --check      # prop models exist and fit their budgets
cargo build --release && python3 -m unittest tests.test_compiled_build
                                          # the compiled executable: fresh install,
                                          # packaged layout, first-run state, config
                                          # reload, malformed content, clean exit
cd level-editor && npm test               # the legacy level editor still parses the catalog
```

## Distribution layout

A packaged build is a directory with the executable and its payload:

```text
Places/
    places                  the executable
    assets/                 catalog.json, levels/, models, textures, decals
        catalog.json
        levels/places_demo.json
        core/ environment/ entities/ diagnostic/
    levels/                 drop-in level packs (*.json and *.zip); created on first run
    import/                 files waiting to be imported; created on first run
    settings.json           written on first run
    cache/                  lightmap cache; created on demand
```

Build one with:

```sh
cargo build --release
tools/package.sh                  # -> target/package/Places and target/package/Places.app
```

`tools/package.sh` also writes the macOS bundle form,
`Places.app/Contents/MacOS/places` with the same payload under
`Contents/Resources/`. Both forms run from any working directory. The asset root
is resolved in this order and the result is printed at startup:

```text
$LIMINAL_ASSET_ROOT                    explicit override (the directory containing assets/)
exe_dir, exe_dir/.. .. exe_dir/../../..   a flat install, and the legacy bin/<triple>/app layout
exe_dir/../Resources                   a macOS .app bundle
assets, ./assets, ../assets            the working directory (development)
$CARGO_MANIFEST_DIR/assets             development builds only, never a release binary
```

A release binary therefore cannot read the source tree it was built from, and a
missing asset root is reported loudly with the full list of locations checked
rather than silently degrading. `assets/levels/` is scanned for shipped levels
and `levels/` for drop-in ones; both appear in the same Level Select menu. The
writable side — `settings.json`, `levels/`, `import/` and `cache/` — always
lives below the package root (the parent of `assets/`), or below
`$LIMINAL_STATE_ROOT` when that is set, never in whatever directory the process
happens to be started from.

## Assets, themes and ids

`assets/catalog.json` is the authoritative registry. Levels reference **logical
ids**, never paths, so a file can move without editing a level. There are two
environment themes, `office` and `pool`; themes document and group content and
never restrict placement — an Office fixture may light a Pool room and an entity
may stand anywhere.

| Office | id |
| --- | --- |
| Wallpaper (maintained / damaged) | `core:wallpaper_yellow_01` / `core:wallpaper_stained_01` |
| Carpet (maintained / damp) | `core:carpet_beige_01` / `core:carpet_damp_01` |
| Suspended ceiling (maintained / stained) | `core:ceiling_panel_01` / `core:ceiling_stained_01` |
| Fluorescent ceiling panel | `core:fluorescent_panel_01` |
| Desk, chair, cabinet, water cooler, vending machine | `core:desk`, `core:chair`, `core:cabinet`, `core:water_cooler`, `core:vending_machine` |

| Pool | id |
| --- | --- |
| Deck, basin, wall tile, ceiling | `core:pool_tile_deck_01`, `core:pool_tile_basin_01`, `core:pool_tile_wall_01`, `core:pool_ceiling_01` |
| Patio table / chair | `core:pool_table`, `core:pool_chair` |
| Curtain (straight / end / corner) | `core:pool_curtain_straight`, `core:pool_curtain_end`, `core:pool_curtain_corner` |
| Ladder | `core:pool_ladder` |
| Guardrail (straight / end / corner) | `core:pool_guardrail_straight`, `core:pool_guardrail_end`, `core:pool_guardrail_corner` |
| Round downlight / wall luminaire | `core:pool_light_round`, `core:pool_light_wall` |
| NO DIVING sign | `core:decal_no_diving_01` |

Generic props (`core:couch`, `core:bed`, `core:table`, appliances, …) carry no
theme and are used by the official demo. `spooner-man` is an `entity`, not
a prop, and places through the same system.

### Textures and materials

A level names a *material*; the catalog maps that material to a *texture* asset
that owns the PNG, plus a world-space tiling period and a static tint. Replace
the PNG under `assets/environment/<theme>/textures/` (or a decal sheet under
`assets/environment/pool/decals/`, `assets/core/decals/`, or a light fixture's
face under `assets/environment/<theme>/textures/lights/`) and restart the game.

Every surface, decal and fixture PNG is an ordinary editable file; the ones
under `tools/` regenerate the shipped set deterministically, but hand-painted
artwork is just as valid. Add a new surface material without touching Rust: add
the PNG, add a `texture` entry and a `material` entry to the catalog, then name
the material from a level. A material may also name a `normal_texture` (a
tangent-space normal map, generated or hand-painted like any other sheet), a
`specular` strength and a `shine` glossiness for its sheen (a level may override
the glossiness per surface), and an `alpha_mode` with an optional `opacity` for
translucency. A light fixture's *mesh* is still code, but its face
is the PNG its catalog entry names. `assets/README.md` documents the catalog
format, the material/texture split and the asset budgets.

**Colour space.** The renderer has no gamma handling: textures are sampled as
authored and multiplied by a display-space baked shade. That is deliberate — the
shipped artwork, the material tints and every lighting constant were calibrated
together in that space. See "Rendering notes" below.

## Levels and creator content

A level is a JSON document. Rooms are rectangles with an optional numeric floor
elevation, a clear height and a ceiling profile; walls are rectangles with
optional per-face materials and openings cut out of them.

```jsonc
{
  "format_version": 1,
  "id": "my_level",
  "name": "My Level",
  "spawn": { "x": 2.0, "z": 5.0, "yaw_degrees": 0.0 },
  "defaults": { "wall": "core:wallpaper_yellow_01",
                "floor": "core:carpet_beige_01",
                "ceiling": "core:ceiling_panel_01" },
  "rooms": [
    { "x": 0.0, "z": 0.0, "width": 9.0, "depth": 7.0,
      "height": 2.7, "floor_y": 0.0 }
  ],
  "walls": [
    { "x": 0.0, "z": 0.0, "width": 9.0, "depth": 0.3,
      "openings": [ { "kind": "window", "offset": 2.0, "width": 2.0,
                      "height": 1.3, "sill": 1.7 } ] }
  ],
  "floor_regions": [
    { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 3.0, "offset_y": -1.5,
      "material": "core:pool_tile_basin_01",
      "edge_material": "core:pool_tile_wall_01" }
  ],
  "ceiling_lights": [
    { "fixture": "core:fluorescent_panel_01", "x": 4.5, "z": 3.5,
      "brightness": 0.7, "color": [1.0, 0.94, 0.82] }
  ],
  "props": [
    { "model": "core:desk", "x": 2.0, "z": 5.0, "rotation_degrees": 0.0,
      "solid": true, "size": [1.6, 0.75, 0.7] }
  ],
  "decals": [
    { "x": 4.5, "z": 2.0, "width": 0.9, "height": 0.9,
      "material": "core:decal_no_diving_01", "surface": "floor" }
  ]
}
```

Drop a level into `levels/` (optionally in a `.zip` pack with its own textures,
see `assets/README.md`) or `assets/levels/`, and it appears in the Level Select
menu. A level that fails validation is skipped and reported on the console
rather than crashing the game. `assets/levels/README.md` indexes the one shipped
level and explains where the regression fixtures live.

**Creating or modifying a map?** `docs/MAP_AUTHORING_GUIDE.md` is the canonical
authoring reference: the currently implemented level format, asset catalog,
materials, textures, props, lighting, validation workflow and common failure
modes.

Notable supported details:

* **Openings** are cut from the wall's minimum corner: `offset` along the wall's
  length, `width` along the wall, `height` above `sill`. `kind` is `door`,
  `window`, `passage` or `vent`; collision follows the geometry, so `sill: 0`
  is walk-through whatever the kind is.
* **Walls are not generated from rooms.** An unenclosed room shows the void
  through the gap, so shell every room you want to walk inside of.
* **A walkable step is 0.4 m.** A larger height difference is solid from the
  lower side and cannot be walked off from the upper side, which is what makes
  region staircases and pool basins safe without any falling physics.
* **Ceilings** are flat by default; `{"kind": "gable", "ridge": "x",
  "ridge_rise": 2.0}` adds a pitched ceiling, and gable-end walls follow the
  slope unless they author their own height.

The bundled editor under `level-editor/` is a browser tool that writes the same
format. It predates the vertical-geometry keys and does not author, preview or
preserve `floor_y`, `floor_regions`, `ceiling` or wall `y`/`mount`; saving such
a level through it drops them, so edit those as JSON.

## Project structure

```text
src/                 the game crate (`liminal-rust`)
    assets.rs        the catalog: ids, classes, themes, resource paths
    level.rs         the level format, geometry rules and the walkable floor
    loader.rs        level discovery, validation, packs, materials resolution
    lighting/        the bake: partition areas, baselines, fixture pools, visibility
    render/          mesh building, packing, culling, decals, fixtures, the GL renderer
    materials/       PNG decode, texture cache, material and decal resolution
    game.rs          player state, movement and collision
    ui.rs            the menu, level select and settings screens
assets/              the shipped content (catalog, levels, models, textures, decals)
levels/              drop-in custom levels and level packs
tools/               asset, texture, prop and level generators and validators
level-editor/        the legacy browser level editor
docs/screenshots/    the images in this README
platforms/           historical PocketCHIP/Vitrallis packaging, not part of the desktop workflow
```

## Current development status

Working and shipped:

* first-person exploration with collision and floor-elevation traversal;
* two environment themes with external PNG surfaces and four external or
  generated decal sheets;
* baked RGB lighting driven by generic engine-level light sources (point,
  rectangle and line shapes) with per-light colour, intensity, range, falloff
  and enabled state; fixtures and props own lights, and neither materials nor
  fixture families imply one;
* baked lightmaps for static world geometry with a quality-profile density, an
  exact vertex-lit fallback, and a deterministic content-keyed cache;
* automatic static-prop occlusion (contact darkening, blocked pools, grounded
  corners) derived from each placed model's own triangles;
* a separate dynamic-object render path proven by a turning washing-machine
  drum in Places Demo;
* true material emission (`emissive`, `emissive_intensity`, `emissive_mask`),
  independent of environmental illumination: a surface or fixture face can read
  fully bright while casting nothing, and a light can cast while nothing glows;
* a lightweight surface response on top of that lighting: an optional normal map
  (`normal_texture`, `normal_strength`), a sheen (`specular`, `specular_color`,
  `shine`), all of it additive and view-dependent — dull paint, plastic,
  metal, glossy tile, linoleum and wet floors read differently without a
  physically based material model;
* real transparency: a material's `alpha_mode` (`opaque`, `cutout` or `blend`)
  decides whether its texture's alpha channel is ignored, alpha-tested, or
  blended in a sorted pass with depth writes off, and window openings can carry
  a `glass` material so a window holds an actual pane instead of being a hole;
* an offscreen scene path: the 3D scene renders into a colour+depth target and
  is presented to the window by a fullscreen quad, with the UI still drawn at
  the drawable's own resolution;
* Full and Low runtime quality profiles that use the same assets, with Low
  downscaling textures once at level load and the profile switchable while
  playing;
* a sectioned pause-menu Settings screen (Graphics, Display, Controls) backed by
  one runtime settings state, with Bloom, Reflections, Lightmaps, VSync,
  Graphics Quality, Window Mode and Resolution changes applying immediately;
* a 1920×1080 windowed desktop default that adapts to the display work area and
  renders through the Retina drawable, never a stretched low-resolution target;
* multi-material / multi-primitive GLB props with per-primitive textures and
  emission;
* wall-boundary lighting isolation, including light through openings;
* rooms with per-room floor elevation, clear height, flat and gable ceilings;
* recessed and raised floor regions with real transition geometry;
* props and entities placed by logical id, with authored collision boxes;
* one official demo level, and compact regression fixtures that back the
  automated tests without shipping;
* a clean packaged distribution and a catalog-driven content pipeline.

Known limitations, all deliberate:

* no gameplay systems — no objectives, inventory, enemies or scripting;
* no realtime dynamic lights and no realtime shadow maps; reflections are a
  static probe baked per level load or at most one half-resolution planar mirror
  per frame, never a dynamic scene reflection;
* no physically based material model: the surface response is a normal map plus
  a view-dependent sheen shaped by `shine` (`0.0` matte .. `1.0` glossy, with an
  optional per-surface override in a level), not a BRDF, and it has no light
  direction to place a highlighted specular from;
* no refraction, no transmission through glass to the lighting bake, no
  per-object alpha on GLB props (a prop's glTF `alphaMode` is not read yet);
* dynamic objects are engine-spawned, not authorable from a level, and are lit
  by one probe of the static bake (no shadows, no self-occlusion);
* lightmaps are baked and cached, never hand-authored, and a bake that cannot
  fit its page budget falls back to vertex lighting;
* emission reaches surfaces and fixture faces; emissive decals and cone/spot
  lights are not implemented yet;
* no animation, no skinning, no water and no swimming; the pool is empty on
  purpose;
* the surface response is drawn at Full quality only: Low keeps the same
  materials, albedo, emission and alpha and leaves the normal/sheen term out;
* floor regions are rectangular and flat: no ramps or sloped regions;
* no traversal between stacked rooms, and no ceiling or floor openings;
* decals cannot cross a floor or ceiling height change, and a gable ceiling
  takes no decals;
* rooms and walls are axis-aligned rectangles only;
* the legacy level editor does not preserve the vertical-geometry keys.

## Rendering notes

The renderer draws the world in one pass: a baked vertex colour multiplied by a
sampled texel, plus a material emission term, plus an optional surface-response
term (`texture2D(u_texture, v_uv) * v_color * light + sheen + reflection + emission`). The
emissive term is added *after* the light multiply, so darkness cannot extinguish
it, and it never becomes illumination — environmental light comes only from the
generic light sources a level places. The response term is a view-dependent
Fresnel sheen scaled by the same baked light (there is no light direction in the
bake, so there is no highlighted specular to place), shaped by the material's
`shine` and an optional normal map perturbing the shading normal: a low-shine
surface keeps a broad grazing sheen and a faint, blurred reflection, a high-shine
one a tight highlight and a recognizable image. A separate decal pass alpha-tests
a cut-out sheet over the surface it belongs to. Lighting is computed once per
level load, never per frame.

The scene is rendered into an offscreen colour+depth target and resolved into the
display image by one fullscreen pass; the HUD is drawn afterwards, on the default
framebuffer, at the drawable's own resolution, so it stays sharp and outside
every post-processing stage. The target tracks the drawable's size and aspect
ratio (nothing is stretched) and falls back to drawing straight into the
framebuffer if it cannot be created — `LIMINAL_NO_OFFSCREEN=1` forces that
fallback path for an A/B comparison.

The resolve stage adds bloom, exposure and a subtle grade, and it is the only
place a scene pixel becomes a display pixel. Bloom is drawn from the world's
**emissive term alone** — never from brightness — so a brightly lit wall cannot
glow; the tone curve is a soft shoulder above 0.75 that leaves the baked
lighting's own contrast untouched below it; and the fog is a scalar mix in the
world shader. Bloom is a **player setting** (Settings → Graphics → Bloom), not
part of the quality profile, so `Full + Bloom Off` and `Low + Bloom On` are both
valid; with Bloom off no emissive or blur pass is submitted and the bloom targets
are released, and with `Low` plus Bloom off the resolve stage is the identity and
presents the scene with the plain copy quad. `LIMINAL_NO_BLOOM=1` /
`LIMINAL_NO_REFLECTIONS=1` still measure each stage alone for a benchmark run.

Reflections are opt-in per material: a **probe** reads a small cubemap baked once
per level load, a **planar** mirror draws a real second view of the level through
the surface's own plane (at most one plane per frame, half resolution, `Full`
only). Both are weighted by the sheen the material already authors, so a rough or
dull surface suppresses its reflection instead of mirroring. Reflections are a
player setting too (Settings → Graphics → Reflections); turning them off removes
the planar pass, the probe bake and the reflection texture binds from the frame
without a level reload.


Transparent surfaces are drawn after everything opaque, sorted back to front by
the distance from the camera to their spatial batch, with depth testing on and
depth writes off; alpha-tested surfaces are drawn with the opaque world through a
separate fragment stage so the opaque pass keeps early depth testing.

Two runtime quality profiles decide how much of an accepted source texture
reaches the GPU. **Full** is the historical Places runtime size (surface,
fixture and decal sheets up to 1024, prop sheets up to 256) and uploads shipped
assets unchanged. **Low** uses the same assets and box-filters each one once at
level load (sheets 256, prop sheets 128, emissive masks 128). Downscaling is a
load-time step that is cached with the texture it produced, never a per-frame
cost, and `"quality"` in `settings.json` (or `LIMINAL_QUALITY=full|low` for one
run) selects the profile. The profile is a selector in Settings → Graphics and
can be changed while playing: the level's GPU textures and lightmap atlas are
rebuilt from the level already resident, with the player, camera and game state
untouched. Low also leaves the optional surface response out and renders the 3D
scene no wider than the historical 480 px reference width: the same level, the
same materials and the same ids, with the optional per-pixel work dropped. Fog,
frame and emission are the same in both profiles; bloom and reflections are
independent player settings, not profile terms.

A decal owns its depth plane by construction, in two halves that level authors
never have to think about:

* `DECAL_SURFACE_OFFSET_M` displaces every decal 0.2 mm along its surface
  normal. That is a real geometric separation, sub-pixel at any practical
  viewing distance, so the base texture cannot win a pixel in the near and mid
  field no matter how the rasteriser fits its plane equations.
* `DECAL_POLYGON_OFFSET` adds a `glPolygonOffset(-1, -4)` bias in the decal
  pass, so the far field and grazing angles stay in front of the parent surface
  after the physical offset is below the depth buffer's resolution. The
  slope-scaled term tracks the interpolation error, which grows with the depth
  slope.

Both are defined once, in `src/render/view.rs`, and applied in one place,
`render::add_decal_quad`, so a decal authored later inherits the fix
automatically.

There is no sRGB or gamma handling, and adding some is not the small fix it
looks like:

* A shader-only "decode both factors, multiply, re-encode" pair is
  **algebraically an identity** — `encode(decode(a) · decode(b)) = a · b` — so it
  cannot change a single multiply. (Verified: patching both fragment shaders to
  do exactly that produced a byte-identical frame.)
* The place a linear pipeline genuinely differs is where the bake **adds**
  terms on the CPU: room baseline + fixture pool + doorway blend. Summing those
  in linear space would darken every fixture pool by roughly 17–28 % on the
  shipped constants and would compress the channel ratios that make the coloured
  rooms read as coloured, so it is a re-calibration of the whole lighting and
  art set, not a correctness toggle.

The investigation, the numbers and the reasoning are recorded in
`tools/bench/notes/` detail in the changelog; the current pipeline is
kept because it is internally consistent and calibrated as a whole.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the full history, including the
distribution, branding, documentation and demo work.

## License

MIT. See [LICENSE](LICENSE). Third-party notices are in
[THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt).
