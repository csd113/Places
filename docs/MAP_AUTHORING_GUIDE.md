# Places Map Authoring Guide

Repository-wide checks: [authoritative desktop verification](VERIFICATION.md).

| Field | Value |
| --- | --- |
| Document status | **Canonical / living.** Update it whenever the authoring contract changes (see [Maintaining This Guide](#maintaining-this-guide)). |
| Level format version documented | `1` (`format_version` in every level JSON) |
| Asset catalog format version documented | `2` (`format_version` in `assets/catalog.json`) |
| Verification | Re-verified against the working tree at version 0.6.0 plus the Home theme and generic architectural pieces. No commit SHA is pinned: the body was checked line-by-line against `src/level.rs`, `src/loader.rs`, `src/assets.rs`, `src/materials/`, `src/render/`, `src/lighting/`, `assets/catalog.json`, `assets/levels/places_demo.json` and `tests/fixtures/levels/*.json`. |
| Checks that must pass before a code or asset change ships | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features`; `python3 tools/assets/validate.py`; `python3 tools/textures/build.py --check`; `python3 tools/props/build.py --check` (see [Validation Workflow](#27-validation-workflow) for what each proves) |
| Primary benchmark level | `assets/levels/places_demo.json` |

> This revision describes the **current** engine: the Full/Low quality
> profiles, the generic engine-level light model, true emissive materials,
> baked lightmaps, surface response, transparency/glass, the offscreen
> presentation path, selective reflections, post-processing and animated
> emissions. Read [Known Implementation Caveats](#known-implementation-caveats)
> before relying on engine limits, and re-run the validation commands after
> pulling new commits.

---

## 1. Purpose

This is the canonical map and environment authoring reference for **Places**. It is
intended for both humans and AI agents. Before creating or substantially modifying a
Places map, read this document. It describes the currently implemented level format,
asset system, materials, textures, props, lighting, placement rules, validation
process, and common failure modes.

**How an agent should use it.** Read the whole document once, then use it as a lookup
while building:

1. [Source of Truth](#2-source-of-truth) — which file wins when documents disagree.
2. [Map-Building Workflow](#3-map-building-workflow) — the required order of work.
3. The format sections (4–21) — the exact JSON contract.
4. The mistake catalogues (24–26) — what goes wrong and how to avoid it.
5. [Validation Workflow](#27-validation-workflow) and [Final Map QA Checklist](#28-final-map-qa-checklist) — do not call a map finished before both are done.
6. [Authoring Recipes](#authoring-recipes) — compact copy-paste procedures.

This guide describes **Implemented Now** capabilities only, unless a section explicitly
says otherwise. Where the engine's design documents describe future work, the guide
lists it as **Not implemented** and never shows planned syntax as usable.

**Places Demo is the benchmark.** `assets/levels/places_demo.json` is the project's
canonical showcase and the technical reference for established authoring patterns.
A new map does not have to resemble it aesthetically — it is not a mandatory visual
template — but when in doubt about how a feature is authored, the demo (and the
regression fixtures in `tests/fixtures/levels/`) is the pattern to cross-check against.
Do not blindly copy mistakes from it either: every pattern here was re-verified against
`src/level.rs`, the loader/validator, the renderer, the tests, and the catalogs.

---

## 2. Source of Truth

When any two sources disagree, resolve in this order:

1. **Runtime implementation** — `src/level.rs` (level schema and geometry rules),
   `src/loader.rs` (validation, discovery, packs), `src/game.rs` / `src/collision.rs`
   (movement and collision), `src/render/` (meshes, decals, fixtures, props,
   reflections, post-processing), `src/lighting/` (bake and lightmaps),
   `src/materials/` + `src/assets.rs` (catalog and materials),
   `src/quality.rs` + `src/settings.rs` (profiles).
2. **Validation and tests** — `src/loader/tests.rs`, `src/level/tests.rs`,
   `src/render/tests.rs`, `src/materials/tests.rs`, `src/assets/tests.rs`,
   `src/props/tests.rs`, `src/collision/tests.rs`, `src/game/tests.rs`,
   `src/lighting/tests.rs`, and the audit modules under `src/` (`surface_audit.rs`,
   `lighting_audit*.rs`, `lighting_isolation.rs`, `lighting_parity.rs`,
   `lighting_partition_audit.rs`, `lighting_vertical_audit.rs`). Tests pin the
   accepted contract.
3. **Asset catalog** — `assets/catalog.json`, plus `assets/README.md`.
4. **Known-good shipped content** — `assets/levels/places_demo.json`,
   `tests/fixtures/levels/*.json`.
5. **This guide** — update it when 1–4 change.
6. **Design proposals** — aspirational
   only. They describe intent, not a contract, and must never be quoted as syntax.

Authoritative paths:

| What | Path |
| --- | --- |
| Level schema | `src/level.rs` |
| Loader / validator | `src/loader.rs` |
| Collision / walkable floor | `src/collision.rs`, `src/game.rs` |
| Mesh generation | `src/render/common/mod.rs`, `src/render/common/geometry.rs` |
| Decals | `src/render/common/decals.rs`, `src/render/common/mod.rs` |
| Fixture geometry | `src/render/common/fixtures.rs`, `src/lighting/tuning.rs` |
| Prop loading | `src/props.rs`, `src/gltf.rs`, `src/render/common/props.rs` |
| Lighting bake | `src/lighting/` (bake, visibility, occlusion, lightmap) |
| Materials / textures | `src/materials/`, `src/assets.rs` |
| Reflections | `src/render/common/reflections.rs` (routing) and `src/render/opengl/reflections.rs` (probe cubemaps), `src/render/opengl/renderer.rs` |
| Post-processing / fog | `src/render/common/postprocess.rs`, `src/render/common/atmosphere.rs`, `src/render/common/framebuffer.rs` |
| Quality profiles / settings | `src/quality.rs`, `src/settings.rs` |
| Catalog | `assets/catalog.json` |
| Benchmark level | `assets/levels/places_demo.json` |
| Regression fixtures | `tests/fixtures/levels/` |

### Quick implemented-vs-unimplemented reference

| Capability | Status |
| --- | --- |
| Rectangular rooms, per-room `floor_y`, `height`, flat/gable ceiling | Implemented |
| Rectangular walls, per-face materials, doors/windows/passages/vents | Implemented |
| `floor_patches` (material-only), `floor_regions` (recess/raise), 0.4 m step rule | Implemented |
| Baked lightmaps for static world geometry (floors, ceilings, walls, reveals, skirts), with the baked-vertex path as the exact fallback | Implemented |
| Baked vertex lighting, 3 fixture families, per-light colour/intensity/range/falloff, partitions, vertical isolation | Implemented |
| Static props occlude baked light (contact darkening, blocked pools), derived from the placed model's own triangles | Implemented |
| A separate dynamic-object render path (per-frame transforms, no rebuild of static geometry or lightmaps) | Implemented (one engine-created demonstration object; not authorable from a level) |
| Generic engine-level lights (point / rect / line) owned by fixtures and props | Implemented |
| Material emission (`emissive`, `emissive_intensity`, `emissive_mask`) and per-fixture `emission` | Implemented |
| Material surface response (`normal_texture`, `normal_strength`, `specular`, `specular_color`, `shine`; legacy `roughness`) | Implemented |
| Material transparency (`alpha_mode`: `opaque` / `cutout` / `blend`, `opacity`, `alpha_cutoff`) | Implemented |
| Opening glazing: a `glass` material fills a window, vent or door aperture with one pane | Implemented |
| Offscreen scene rendering presented by a fullscreen quad, UI at drawable resolution | Implemented |
| Selective reflections: per-material `reflection_mode` (`none` / `probe` / `planar`) at 64-texel probes (32 on Low) and a half-resolution planar pass | Implemented |
| Restrained post-processing: emission-driven bloom, a tone shoulder, distance fog and a subtle grade, with the UI drawn outside it | Implemented (engine-global; not level-authorable) |
| Animated emissions: `animated_emissions[]` makes a material's emission pulse or flicker, deterministically | Implemented |
| Full / Low runtime quality profiles over the same level content | Implemented |
| Props/entities from GLBs by logical id, `solid` collision boxes | Implemented |
| Multi-primitive / multi-material GLB props, embedded emissive materials, node transforms | Implemented |
| External PNG surfaces, decals, fixture faces; catalog + themes | Implemented |
| Level `.zip` packs with `materials.json` and pack textures | Implemented |
| `ceiling_lights` accepting the `lights` alias | Implemented |
| Water, refraction/transmission, realtime dynamic lights, realtime shadow maps, skeletal animation | Not implemented |
| Screen-space reflections; per-frame raytraced reflections; cubemap probes with realtime updates | Not implemented (static probes and one planar plane exist) |
| Per-object transparency on GLB props (a prop's glTF `alphaMode` is not read) | Not implemented |
| Emissive decals; per-placement emission overrides; cone/spot lights | Not implemented |
| Authoring a normal map from a level (a level names a material, and the material owns the map) | Implemented (via the catalog) |
| Sloped floors (`ramps`), staircases (`stairs`), half walls, columns, archways, guardrails, thresholds, baseboards | Implemented |
| Ceiling/floor openings; traversal between stacked storeys | Not implemented |
| Room-wide brightness/tint modifiers; non-fixture decor meshes beyond props | Not implemented |
| WebP or formats other than PNG; arbitrary structural meshes | Not implemented |
| `wall_lights` level array; per-room wall material | Not implemented (wall fixtures live in `ceiling_lights` with `"mount": "wall"`; prop-owned lights live in `props[].lights`) |

---

## 3. Map-Building Workflow

Follow this order. Do not start geometry before steps 1–3, and do not finish before
steps 14–17.

1. **Understand the requested environment.** Restate what the map must contain:
   rooms, route, mood, lighting, props, signs. Identify which existing theme(s) it is
   closest to (`office`, `pool`, or generic shared `core:`/`environment` assets).
2. **Inspect available catalog assets.** Read `assets/catalog.json` (or the tables in
   [Asset Catalog](#14-asset-catalog)) for materials, textures, props, decals and
   lights that already fit. Prefer reuse over creation.
3. **Plan the layout.** Sketch the rooms as rectangles with world coordinates: where
   the player spawns, the route, which rooms share walls, where elevations change.
4. **Choose elevations and ceiling heights.** `floor_y` per room, `height` per room,
   `floor_regions` for stairs/platforms/recesses. Remember the 0.4 m walkable step.
5. **Plan openings and connections.** Decide each door/window/passage/vent, its wall,
   offset, width, height, sill. Remember: rooms do **not** generate walls; shell every
   room the player can enter.
6. **Choose materials.** Name the intended logical material ids for floors, ceilings,
   walls and wall faces. Check they exist in the catalog.
7. **Identify missing assets.** New surface appearance? New decal? New prop? New light
   fixture? New theme? Use [Building New Assets for a Map](#23-building-new-assets-for-a-map).
8. **Create missing assets correctly.** PNGs first, catalog entries second, and only
   then reference them from the level. Never generate normal game textures in code
   (see [Textures](#12-textures)).
9. **Register assets in the catalog.** Add the entries to `assets/catalog.json`
   following [Asset Catalog](#14-asset-catalog) and the recipes.
10. **Author structural geometry.** Rooms → walls → openings → floor patches/regions.
    Check wall min-corner placement and opening bounds as you go.
11. **Add props.** Logical ids, x/y/z, rotation, `scale`, `size` for solid props.
    Mind +Z fronts. Add `props[].lights` only when the object should illuminate.
12. **Add decals.** One flat surface each; never across height changes; no gable ceilings.
13. **Add lights.** Choose fixture types per intended look; place enough fixtures to
    light the route; use colour/intensity for mood. Wall fixtures need `mount`+`y`.
14. **Check collision.** `solid` props, door headers, window sills, unwalkable rims.
    Walk the route in-game mentally against the 0.4 m step rule.
15. **Inspect for overlapping/coplanar geometry.** No duplicated floors or walls;
    doorway thresholds owned once; no wall ends buried in other walls.
16. **Validate assets.** Run the catalog, texture and prop checks
    ([Validation Workflow](#27-validation-workflow)).
17. **Run tests and boot the level.** `cargo test --workspace --all-features`, then
    `PLACES_LEVEL=<id> cargo run` and read the console. A level that fails validation
    is reported as `[levels] skipping …` at discovery, so read the console even when
    the level is meant to appear in the menu.
18. **Visual/render validation if available.** Screenshot with
    `PLACES_CAPTURE=frame.png PLACES_LEVEL=<id> cargo run` and inspect: no holes, no
    flicker, no light leaks, no floating props.

**Worked example.** "An abandoned hotel with a flooded basement and dim green emergency
lights" resolves to: read this guide → check the catalog (no hotel theme exists yet, so
add one per the theme recipe; generic `core` props such as `core:couch`, `core:sink`,
`core:bed` already fit a hotel) → create hotel wall/floor/ceiling PNGs + materials →
one `rooms` entry at `floor_y: 0.0` for the lobby and a `floor_y: -3.0` room for the
basement (reach it the way Places Demo reaches its stair hall: a chain of
`floor_regions` whose offsets differ by ≤ 0.4 m, with the topmost region meeting the
doorway) → reuse `core:carpet_damp_01` for flood-damaged surfaces (there is **no water
rendering**, so "flooded" must be implied by damp/stained materials and region
recesses) → dim green lights as ordinary ceiling fixtures with
`"color": [0.35, 1.0, 0.45]` and low `brightness` → validate. Do **not** author a
second variant of the level for the Low quality profile; one level is used by both.

---

## 4. Coordinate System and Units

| Item | Value |
| --- | --- |
| Unit | **1.0 = 1 metre.** No scale factor anywhere in the level path. |
| Handedness | Right-handed, **Y up** (`perspective_rh_gl`). |
| Horizontal plane | X/Z; world Y is up. |
| Compass | **−Z = north, +Z = south, −X = west, +X = east** (wall face names, decal surface names and gable `ridge` all use this). |
| Origin | World origin is arbitrary; floors are commonly authored at `y = 0.0`. A room's `floor_y` sets its own floor elevation. |
| Yaw | Degrees. **0° faces −Z (north); +90° faces +X (east); +180° faces +Z; +270° faces −X.** Forward vector is `(sin yaw, 0, −cos yaw)`, so yaw increases turning right (clockwise seen from above with north up). |
| Spawn | `{ "x": …, "z": …, "yaw_degrees": … }`. There is **no spawn `y`**; the eye is placed at the walkable floor under `(x,z)` plus 1.6 m. |
| Player | radius 0.3 m, height 1.8 m, eye 1.6 m; **walkable step = 0.4 m**. |
| Room tolerance | A point within **0.01 m** of a room's footprint edge counts as inside it. |

```text
                    -Z  north
                     ^
                     |
     west  -X  <-----+----->  +X  east
                     |
                     v
                    +Z  south

     Y is up, out of the floor toward the ceiling.
     yaw 0 looks north (-Z).  yaw +90 looks east (+X).
     Wall face names: north (-Z), south (+Z), west (-X), east (+X).
     Wall face names are normals, not directions of travel.
```

Consequences to internalise:

* `offset` on a wall opening grows along **+X** on an X-axis wall and **+Z** on a
  Z-axis wall, starting at the wall's minimum corner.
* Decal `surface` values are normals: a floor decal has normal +Y, a ceiling decal −Y,
  `wall_north` −Z, `wall_south` +Z, `wall_west` −X, `wall_east` +X.
* A prop at `rotation_degrees: 0` faces **+Z**; the model is authored that way
  (`assets/README.md`, "Prop and entity conventions").
* A prop's `y` is an offset **above the local walkable floor**, while a wall or
  fixture `y` is a **world** height. The two conventions differ and are not
  interchangeable.

---

## 5. Level File Structure

A complete level is one JSON object. This is a **readable subset** of the format that
shows the shape and the common fields; the complete field-by-field contract is in the
tables of sections 6–10, 16, 17 and 19–21, and the skeleton below names every current
field at least once (the legacy `room` key is covered in prose below it). Unknown keys
are **silently ignored** (the structs do not use
`deny_unknown_fields`), so a typo disappears without an error; diff against the
skeleton and the per-field tables.

```jsonc
{
  "format_version": 1,                     // REQUIRED. Must be exactly 1.
  "id": "my_level",                        // REQUIRED. Non-empty; menu key.
  "name": "My Level",                      // REQUIRED. Non-empty; display name.
  "author": "",                            // optional, default "".

  "spawn": { "x": 2.0, "z": 5.0, "yaw_degrees": 0.0 },  // x, z REQUIRED; yaw default 0.0

  "defaults": {                            // optional block; see the warning below
    "wall": "core:wallpaper_yellow_01",    // optional per-surface shine overrides:
    "floor": "core:carpet_beige_01",       // wall_shine / floor_shine / ceiling_shine
    "ceiling": "core:ceiling_panel_01"
  },

  "rooms": [                               // floors + ceilings only; no implicit walls
    {
      "x": 0.0, "z": 0.0,                  // optional, default 0.0 each
      "width": 9.0, "depth": 7.0,          // REQUIRED, > 0
      "height": 2.7,                       // optional, default 4.0
      "floor_y": 0.0,                      // optional, default 0.0
      "ceiling": { "kind": "flat" },       // optional, default flat
      "material": "core:carpet_beige_01",  // optional, default defaults.floor
      "shine": 0.0,                        // optional 0..1; default = material's own
      "ceiling_material": "core:ceiling_panel_01", // optional, default defaults.ceiling
      "ceiling_shine": 0.0                 // optional 0..1; default = material's own
    }
  ],

  "walls": [
    {
      "x": 0.0, "y": 0.0, "z": 0.0,        // x, z REQUIRED; y default 0.0
      "width": 9.0, "depth": 0.3,          // REQUIRED, > 0
      "height": 2.7,                       // optional: omitted follows the local ceiling
      "material": "core:wallpaper_yellow_01",          // optional, default defaults.wall
      "shine": 0.0,                        // optional 0..1 for this wall's faces
      "faces": { "north": "core:wallpaper_stained_01" }, // optional per-face overrides
      "face_shine": { "north": 0.0 },      // optional per-face 0..1, keyed like faces
      "openings": [
        {
          "kind": "door",                  // optional, default "door"; free string
          "offset": 2.0,                   // REQUIRED
          "width": 1.2,                    // REQUIRED
          "height": 2.1,                   // REQUIRED
          "sill": 0.0,                     // optional, default 0.0
          "glass": "core:glass_window_clear_01",  // optional; absent = bare hole
          "glass_shine": 0.2               // optional 0..1 for the pane
        }
      ]
    }
  ],

  "floor_patches": [                       // material-only overlays (no elevation)
    { "x": 3.0, "z": 5.0, "width": 2.0, "depth": 1.5,
      "material": "core:carpet_damp_01",   // required
      "shine": 0.0 }                       // optional 0..1
  ],

  "floor_regions": [                       // recesses / raised platforms
    {
      "x": 2.0, "z": 2.0, "width": 4.0, "depth": 3.0,  // REQUIRED
      "offset_y": -1.5,                    // optional, default 0.0
      "material": "core:pool_tile_basin_01",           // optional
      "shine": 0.3,                        // optional 0..1
      "edge_material": "core:pool_tile_wall_01",       // optional
      "edge_shine": 0.28                   // optional 0..1
    }
  ],

  "ramps": [                               // sloped walking surfaces
    { "x": 4.0, "z": 0.4, "width": 1.0, "depth": 1.6,  // REQUIRED
      "offset_y": 0.0, "rise": 0.75,        // offset at the min corner, signed rise
      "material": "home:hardwood_oak_01",   // optional: top surface
      "shine": 0.3,
      "edge_material": "home:wall_paint_offwhite_01",  // optional: side faces
      "edge_shine": 0.2 }
  ],

  "stairs": [                              // straight stepped flights
    { "x": 2.2, "z": 2.6, "width": 1.4, "depth": 1.2,  // REQUIRED
      "offset_y": 0.0, "rise": 0.75, "steps": 5,       // REQUIRED rise and steps
      "material": "home:hardwood_oak_01",   // treads (default: room floor)
      "riser_material": "home:wall_paint_offwhite_01", // risers (default: treads)
      "side_material": "home:baseboard_white_01" }     // sides  (default: risers)
  ],

  "half_walls": [                          // capped knee walls
    { "x": 6.05, "z": 6.6, "width": 1.0, "depth": 0.2,  // min corner like a wall
      "height": 1.05,                      // REQUIRED
      "y": null,                           // optional absolute base; floor default
      "material": "home:wall_paint_offwhite_01",
      "end_material": null, "cap_material": "home:baseboard_white_01" }
  ],

  "columns": [                             // square / rectangular posts
    { "x": 3.7, "z": 2.1, "width": 0.26, "depth": 0.26,
      "height": null,                      // default: floor to local ceiling
      "material": "home:wall_paint_offwhite_01",
      "cap_material": "home:baseboard_white_01" }
  ],

  "archways": [                            // wall block with an arched opening
    { "x": 5.83, "z": 1.6, "width": 0.34, "depth": 1.4,
      "height": 3.0,                       // block height
      "opening_width": 1.0,                // centred in the block's length
      "opening_height": 2.1,               // clear height at the crown
      "arch_rise": 0.25,                   // crown above the springing line
      "material": "home:wallpaper_offwhite_01",
      "reveal_material": "home:wall_paint_offwhite_01" }
  ],

  "guardrails": [                          // rails and stair handrails
    { "x": 5.35, "z": 2.1, "length": 2.2,  // start point, run along local +X
      "rotation_degrees": 270.0,           // 0 east, 90 north, 180 west, 270 south
      "height": 0.95, "rise": null,        // omitted: follow the walkable floor
      "post_spacing": 1.2,
      "material": "home:handrail_wood_01", "post_material": null }
  ],

  "thresholds": [                          // floor transition strips
    { "x": 6.0, "z": 2.3, "length": 1.04,  // centre, run along local +X
      "thickness": 0.08, "height": 0.012, "rotation_degrees": 90.0,
      "material": "home:threshold_wood_01" }
  ],

  "baseboards": [                          // skirting runs
    { "x": 0.15, "z": 0.15, "length": 5.85, // start point on the wall's face
      "rotation_degrees": 0.0, "height": 0.09, "thickness": 0.018,
      "material": "home:baseboard_wood_01" }
  ],

  "decals": [
    { "x": 4.5, "y": 0.0, "z": 2.0,        // x, z REQUIRED; y default 0.0
      "width": 0.9, "height": 0.9,         // REQUIRED, > 0, <= 10
      "rotation_degrees": 0.0,             // optional, default 0.0
      "material": "core:decal_no_diving_01",  // REQUIRED
      "surface": "floor" }                 // REQUIRED enum
  ],

  "ceiling_lights": [                      // ALL fixtures, ceiling and wall; alias: "lights"
    {
      "fixture": "core:pool_light_wall",   // REQUIRED
      "x": 0.15, "z": 13.0,                // REQUIRED
      "rotation_degrees": 90.0,            // optional, default 0.0
      "brightness": 0.7,                   // optional; alias "intensity"; default 1.0
      "color": [0.55, 0.78, 1.0],          // optional; default [1.0, 0.96, 0.88]
      "mount": "wall",                     // optional; default "ceiling"
      "y": 1.9,                            // REQUIRED when mount is "wall"
      "range": 6.0,                        // optional, default 6.0
      "falloff": "smooth",                 // optional, default "smooth"
      "enabled": true,                     // optional, default true
      "emission": 0.7                      // optional; default = brightness
    }
  ],

  "props": [
    {
      "model": "core:desk",                // REQUIRED
      "x": 2.0, "y": 0.0, "z": 5.0,        // optional, default 0.0; y is floor-relative
      "rotation_degrees": 0.0,             // optional, default 0.0
      "scale": 1.0,                        // optional, default 1.0, must be > 0
      "size": [1.6, 0.75, 0.7],            // optional [w,h,d]; collision box, x scale
      "solid": true,                       // optional, default false
      "lights": [                          // optional, default []; max 8 per prop
        {
          "shape": "rect",                 // optional; default "point"
          "half_width": 0.3, "half_depth": 0.05,  // used by "rect"
          "length": 1.2,                   // used by "line"
          "offset": [0.0, 0.9, 0.3],       // optional, default [0, 0, 0]
          "rotation_degrees": 0.0,         // optional, default 0.0
          "color": [0.55, 0.78, 1.0],      // optional; default [1.0, 0.96, 0.88]
          "intensity": 0.15,               // optional; alias "brightness"; default 1.0
          "range": 3.0,                    // optional, default 6.0
          "falloff": "smooth",             // optional, default "smooth"
          "enabled": true                  // optional, default true
        }
      ]
    }
  ],

  "animated_emissions": [
    {
      "material": "core:glass_sign_lit_01",  // REQUIRED
      "effect": "pulse",                   // optional; `pulse` or `flicker`; default pulse
      "hz": 0.09,                          // optional; effect default when absent
      "depth": 0.18,                       // optional; effect default when absent
      "phase": 0.0                         // optional, default 0.0
    }
  ]
}
```

Two collections are merged/legacy and should not be used in new maps except for
compatibility: `room` (a single `RoomDef`, resolved **after** the `rooms` array) and
the catalog's legacy `props` array (see [Asset Catalog](#14-asset-catalog)).

**Warning — the `defaults` gotcha.** If the `defaults` key is absent entirely, the
engine uses `core:wallpaper_yellow_01` / `core:carpet_beige_01` /
`core:ceiling_panel_01`. If you author `defaults`, author **all three keys**: each
missing key becomes the empty string, and an empty material id resolves to the
*untextured white sheet*, not the built-in default. Never leave a material id empty.

**Closed enums vs free strings.** Serde rejects the whole document at parse time when a
*closed enum* field has an unknown value: `ceiling.kind`, `ridge`, `mount`, `falloff`,
`shape` (prop light), and decal `surface`. Free strings are validated later by the
loader or the renderer: opening `kind` (unknown names load), animated-emission
`effect` (unknown names are a named validation error), and all logical asset ids.
A misspelled enum is a parse error, not a silently ignored key.

**Where levels live.** `assets/levels/*.json` ships with the game;
`levels/*.json` and `levels/*.zip` are drop-in packs (under the writable state root,
normally next to the asset root). Both appear in the Level Select menu.
`tests/fixtures/levels/` is for engine regression fixtures and is never packaged.
A level file that fails to read, parse or validate is reported at discovery as
`[levels] skipping {path}: {reason}`, so check the console rather than assuming it is
absent. `PLACES_LEVEL=<id>` boots a specific level and prints validation errors
verbatim.

### Level limits

The engine enforces several independent caps. Only some of them reject a level:
read the "Enforced as" column carefully.

| Limit | Value | Enforced as |
| --- | --- | --- |
| Rooms (`rooms` + legacy `room`) | ≤ 500 | Loader rejection: `Level contains too many rooms: …` |
| Walls | ≤ 5000 | Loader rejection |
| Ceiling lights | ≤ 5000 | Loader rejection |
| Props | ≤ 5000 | Loader rejection |
| Decals | ≤ 5000 | Loader rejection |
| Decal edge (`width`, `height`) | ≤ 10 m | Loader rejection |
| Floor regions | ≤ 2000 | Loader rejection |
| Ramps | ≤ 500 | Loader rejection |
| Staircases | ≤ 500 | Loader rejection |
| Half walls | ≤ 2000 | Loader rejection |
| Columns | ≤ 2000 | Loader rejection |
| Archways | ≤ 500 | Loader rejection |
| Guardrails | ≤ 2000 | Loader rejection |
| Thresholds | ≤ 1000 | Loader rejection |
| Baseboards | ≤ 2000 | Loader rejection |
| Floor patches | ≤ 2000 | Loader rejection |
| Openings per wall | ≤ 64 | Loader rejection on that wall |
| Room width/depth | ≤ 2000 m | Loader rejection per room |
| Room height | ≤ 50 m | Loader rejection per room |
| Gable `ridge_rise` | ≤ 50 m, and > 0 | Loader rejection per room |
| Estimated floor area | ≤ 1 000 000 m² | Loader rejection (its own estimate, computed from room rectangles) |
| Estimated generated vertices | ≤ 2 000 000 | Loader rejection (its own upper-bound estimate) |
| Standalone level JSON file size | ≤ 8 MiB (`8 * 1024 * 1024` bytes) | Rejected before parsing; the embedded fallback demo is exempt |
| ZIP pack: entries / entry size / total uncompressed | ≤ 500 entries / ≤ 10 MB per entry / ≤ 50 MB total | Pack rejected while reading |
| Distinct prop models placed | ≤ 256 | **Not a rejection:** later placements draw placeholder boxes |
| Summed prop vertices (after instancing) | ≤ 1 500 000 | **Not a rejection:** further placements draw placeholder boxes |

The prop-model caps (triangles, vertices, primitives, materials, images, texture edge)
are listed in [Props and Models](#16-props-and-models); an over-budget *model* falls
back to a placeholder box with a one-time `[props]` warning.

---

## 6. Level Metadata and Spawn

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `format_version` | integer | **yes** | — | Must be `1`; anything else is rejected: `Unsupported level format_version: {v} (expected 1)`. |
| `id` | string | **yes** | — | Non-empty after trim. Not checked for uniqueness across files (see caveats). |
| `name` | string | **yes** | — | Non-empty after trim. |
| `author` | string | no | `""` | Display metadata only. |
| `spawn.x` | number | **yes** | — | World X. Must be finite. |
| `spawn.z` | number | **yes** | — | World Z. Must be finite. |
| `spawn.yaw_degrees` | number | no | `0.0` | 0 = north (−Z), +90 = east (+X). Must be finite when present. |

Known-valid header (from Places Demo):

```json
{
  "format_version": 1,
  "id": "places_demo",
  "name": "Places Demo",
  "author": "Places",
  "spawn": { "x": 2.0, "z": 5.6, "yaw_degrees": 74.0 }
}
```

* The engine does **not** require the spawn to be inside a room. Outside every room
  the walkable floor silently falls back to `y = 0.0`, so the player boots at eye
  height 1.6 over the void. Always check this yourself.
* Default material ids if `defaults` is omitted:
  wall `core:wallpaper_yellow_01`, floor `core:carpet_beige_01`,
  ceiling `core:ceiling_panel_01`. If `defaults` is present, author all three keys
  (see the warning in section 5).

---

## 7. Rooms

A room defines only a **floor plane and a ceiling volume** over a rectangle. It
generates **no walls**: an unenclosed room shows the void through the gap. Every space
the player should walk inside must be enclosed by authored `walls`.

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | no | `0.0` | Minimum corner of the footprint (normalised; `width`/`depth` may be negative at parse but validation rejects ≤ 0). |
| `width` | number | **yes** | — | X extent, `> 0`, `≤ 2000` m. |
| `depth` | number | **yes** | — | Z extent, `> 0`, `≤ 2000` m. |
| `height` | number | no | **`4.0`** | Clear floor-to-eave height, room-local, `> 0`, `≤ 50` m. A gable adds `ridge_rise` above the eave. |
| `floor_y` | number | no | `0.0` | World Y of the room's floor plane. Moves floor, walls and ceiling together. |
| `ceiling` | object | no | `{"kind":"flat"}` | Ceiling profile; see below. |
| `material` | string | no | `defaults.floor` | Floor material override for this room. |
| `shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for `material`. |
| `ceiling_material` | string | no | `defaults.ceiling` | Ceiling material override for this room. |
| `ceiling_shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for `ceiling_material`. |

There is no per-room wall material: walls carry their own `material` / `faces`.

```json
{ "x": 0.0, "z": 0.0, "width": 9.0, "depth": 7.0, "height": 2.7 }
```

Places Demo, a room raised/lowered as a unit (stair hall, `floor_y: -1.5` means a
1.5 m drop from the office floor; the route reaches it by stairs built from floor
regions):

```json
{ "x": 19.0, "z": 0.0, "width": 5.0, "depth": 7.0, "height": 4.2,
  "floor_y": -1.5, "material": "core:carpet_damp_01",
  "ceiling_material": "core:ceiling_stained_01" }
```

### Ceiling profiles

Ceiling profiles are tagged objects. A flat ceiling:

```json
{ "kind": "flat" }
```

A gable ceiling with the ridge running along X and a 2 m rise above the eave:

```json
{ "kind": "gable", "ridge": "x", "ridge_rise": 2.0 }
```

* Tagged enum: `kind` is `"flat"` or `"gable"`; an unknown `kind` is a JSON parse error.
* `ridge` is the axis the ridge runs **along**: `"x"` leaves the ridge constant in X
  and slopes the ceiling along Z; `"z"` is the mirror case. The ridge sits at the
  footprint midpoint of the perpendicular axis.
* `ridge_rise` is metres above the eave; it must be finite, `> 0` and `≤ 50`, or the
  level is rejected (`Room {i} ceiling ridge rise …`).
* Gable ceilings are real sloped geometry (two slopes meeting at a ridge cut line). A
  wall whose `height` is omitted follows the local ceiling, including splitting at a
  crossing ridge; a wall with an authored `height` is rigid and can poke through.
* Gable-end walls follow the slope unless they author their own height.
* **No decals on gable ceilings** and no ceiling openings.

### Room overlap and ownership

Overlapping rooms are legal and sometimes intentional (they are how stacked storeys
and vertical features are built). Two different ownership rules apply:

* **Geometry, collision and the walkable floor** use the **first room in resolution
  order** (`rooms` order, then the legacy `room` entry) whose footprint contains the
  point (0.01 m tolerance). If several rooms overlap, the earlier one wins.
* **Baked lighting** uses the **smallest-area** room at that point when no height hint
  is authored; an authored light `y` first picks the room whose vertical air volume
  contains that height, so a fixture on one storey does not lend its power to the
  other. Ties break to the smaller area, then to the earlier room.

Both floors/ceilings of an intentional overlap are emitted. This mismatch is a known
design property, not a bug; keep overlapping footprints deliberate and minimal.

### Rooms and lighting

Each room (or, with internal partitions, each partitioned area) contributes a
**baseline** to its surfaces from the fixture power it owns. A fixture-free room sits
at the neutral ambient floor `0.10` — unlit rooms are dark by design. See
[Lighting](#18-lighting).

---

## 8. Walls

A wall is a rectangle in plan plus a base height and an optional authored height.
Rooms do not create walls; walls are placed by their **minimum corner**.
`python3 tools/assets/validate.py` warns when a wall's footprint touches no room,
which almost always means an authored-by-centre mistake.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Minimum corner of the footprint. |
| `y` | number | no | `0.0` | **Absolute world Y of the wall base** (not room-relative). |
| `width` | number | **yes** | — | X extent, `> 0`. |
| `depth` | number | **yes** | — | Z extent, `> 0`. |
| `height` | number | no | follows the local ceiling | Authored height above `y`. Omitted = the wall top follows the room ceiling (gable-aware). Authored = rigid. |
| `material` | string | no | `defaults.wall` | Object-level material for the wall's length faces. |
| `shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for the wall's faces that draw `material`. |
| `faces` | object | no | `{}` | Per-face material overrides; wins over `material`. |
| `face_shine` | object | no | `{}` | Per-face glossiness overrides, keyed like `faces`; a face with a different material keeps that material's default. |
| `openings` | array | no | `[]` | Rectangular cutouts (≤ 64 per wall); see [Openings](#9-openings). |

**Axis, length, thickness.** A wall's **length axis** is the larger of `width`/`depth`
(ties → X). Its **length** is that extent; its **thickness** is the other extent. So
"depth is the thickness" only for an X-axis wall; for a Z-axis wall the thickness is
`width`.

**Min-corner anchor.** The footprint is `x..x+width × z..z+depth` (normalised).
Opening offsets are measured from the minimum corner (`x.min(x+width)`,
`z.min(z+depth)`) along +X (X-axis wall) or +Z (Z-axis wall).

**Default height.** With `height` omitted, the wall top is the local clear ceiling at
each point — which is why an unheighted wall in a gable room follows the slope. A wall
spanning two rooms of different ceiling heights follows the ceiling above each part.
With `height` authored, the wall is drawn exactly `y..y+height`, even through a
ceiling or across rooms. Validation requires a positive finite `height` when it is
present.

**Face names.** Read the names as normals:

| Wall axis | Valid `faces` keys | Notes |
| --- | --- | --- |
| X-axis (length along X) | `"north"` (normal −Z, low-Z face), `"south"` (normal +Z, high-Z face) | `"west"`/`"east"` keys are ignored. |
| Z-axis (length along Z) | `"west"` (normal −X, low-X face), `"east"` (normal +X, high-X face) | `"north"`/`"south"` keys are ignored. |

Material precedence for each length face: `faces[<face>]` → `wall.material` →
`defaults.wall`. Door/window reveals and caps use the same precedence.

```json
{ "x": 8.85, "z": 0.15, "width": 0.3, "depth": 6.7, "y": 0.0, "height": 2.7,
  "openings": [ { "kind": "door", "offset": 2.85, "width": 1.2,
                  "height": 2.1, "sill": 0.0 } ] }
```

Places Demo, per-face material on a Z-axis wall (the office side of the shared pool
wall keeps office wallpaper while the pool side is tile):

```json
{ "x": 18.85, "z": 0.15, "width": 0.3, "depth": 6.7, "y": -1.5, "height": 4.2,
  "material": "core:wallpaper_stained_01",
  "faces": { "west": "core:wallpaper_yellow_01" } }
```

### Fixed wall shading

The renderer folds a small fixed directional shade into every wall face, before baked
light is applied. It is not authorable and applies on both the lightmapped and the
vertex-lit path, so it is worth knowing when comparing two walls:

| Surface | Multiplier |
| --- | --- |
| Wall face with normal −Z (north) | 1.00 |
| Wall face with normal +Z (south) | 0.88 |
| Wall face with normal −X (west) | 0.84 |
| Wall face with normal +X (east) | 0.94 |
| Door/window jamb reveal | 0.78 |
| Door/window header reveal | 0.92 |
| Bottom of a wall face | ×0.92 of its face value |
| Top of a wall face | ×1.05 of its face value |

### Avoiding duplicate coplanar surfaces

This is the single most common geometry failure. Two walls that share a plane,
thickness and vertical span and overlap in length **and** height are resolved into one
emission unit: the group's solid profile is unioned and its surfaces carry material
runs, where the **last covering wall in authored order owns each length×height cell**;
hidden end caps and reveals are clipped. That prevents most z-fighting but it is not a
licence to duplicate:

* Never place two walls with the same footprint or the same length-face plane
  "because it renders the same". Overlap makes materials ambiguous and can surface as
  flicker at angles.
* Never continue a wall by starting a second wall at the same base/height over a
  different length without reason; the renderer will merge them, but authored overlap
  is fragile.
* Wall end caps/reveals are clipped against abutting walls; a wall end buried in
  another wall contributes nothing visible.
* A coalesced group emits one **pane per authored opening**, so two coincident walls
  with the same `glass` opening each draw their own pane. Author each physical wall
  once.
* The surface audit (`cargo test surface_audit`) checks Places Demo and fixed cases.
  There is no general z-fighting detector for your level; inspect junctions manually
  and keep one physical wall per surface.

---

## 9. Openings

An opening is a rectangular hole cut through a wall's thickness. All opening kinds
are the same rectangle; `kind` only changes labels and one lighting behavior.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `kind` | string | no | `"door"` | Free string; documented spellings `door`, `window`, `passage`, `vent`. Unknown strings load (forward compatibility) but are not doors for lighting. |
| `offset` | number | **yes** | — | Distance along the wall's length axis from the min corner to the opening's near edge; `≥ 0`. |
| `width` | number | **yes** | — | Cut width along the wall; `> 0`; `offset + width ≤ length` (tolerance 1e-3) or the level is rejected. |
| `height` | number | **yes** | — | Cut height above the sill; `> 0`. |
| `sill` | number | no | `0.0` | Bottom edge above the wall's base (`wall.y`); `≥ 0`. `0.0` reaches the floor. |
| `glass` | string | no | — | Material id of a pane filling the aperture. Absent (or blank) = the historical bare hole. See [Panes](#panes-glass-grilles-and-screens). |
| `glass_shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for the `glass` pane. |

```text
   X-axis wall: footprint (x .. x+width) by (z .. z+depth)
   length axis = X, length origin = min corner (x, z)

   (x,z)                                  (x+width, z)
     +------------------+----+------------------+
     |                  |    |                  |  <- "north" face (normal -Z)
     |                  |    |                  |
     +------------------+----+------------------+
                        ^    ^
                  offset┘    └ offset + width
                        <----> opening width
   thickness = depth (z .. z+depth)
```

### Door

A walk-through hole when `sill: 0.0`. `kind: "door"` (and `"passage"`) also get the
bounded **doorway baseline light blend** between the connected rooms, but only when the
opening's bottom reaches the lower of the two connected floors
(`wall.y + sill <= min(floor of both sides) + 1e-3`). A raised-sill door on a raised
wall base does not blend.

```json
{ "kind": "door", "offset": 2.85, "width": 1.2, "height": 2.1, "sill": 0.0 }
```

A raised-sill door is legal and common for transitions between different elevations.
Places Demo uses this to descend into the corridor: `sill: 0.6` on a wall whose base
is already 1.5 m below (the opening's real floor edge sits at wall.y + sill).

```json
{ "kind": "door", "offset": 1.65, "width": 1.6, "height": 2.1, "sill": 0.6 }
```

### Window

Same rectangle; practically a hole with `sill > 0`, and collision follows the
geometry so a raised window blocks the player and transmits light over the sill.
**Windows do not blend room baselines** — they transmit fixture pools through the
aperture only. Do not rely on a window to make a dark room borrow its neighbour's
brightness.

```json
{ "kind": "window", "offset": 1.65, "width": 3.2, "height": 1.3, "sill": 1.7 }
```

A window with `glass` really is glazed, which is the usual way a Places room gets
a window you can look *through* rather than *into*:

```json
{ "kind": "window", "offset": 1.65, "width": 3.2, "height": 1.3, "sill": 1.7,
  "glass": "core:glass_window_dirty_01" }
```

### Passage

A walk-through hole with `kind: "passage"`. Unlike `window`/`vent`, it receives the
same doorway baseline blend as a door and is the right label for wide openings and
room-to-room thresholds without doors.

```json
{ "kind": "passage", "offset": 0.15, "width": 2.6, "height": 2.4, "sill": 0.0 }
```

### Vent

Documented spelling with no special geometry. Pools transmit through it exactly like
any hole; it does **not** receive the doorway baseline blend. Use it for small high or
low service openings.

```json
{ "kind": "vent", "offset": 1.0, "width": 0.6, "height": 0.4, "sill": 2.4 }
```

### Opening interactions you must respect

* **Walls are not cut down to the floor automatically.** An opening's bottom is
  `wall.y + sill`; the doorway floor must be provided by the rooms' floors or by a
  floor region. Placing a door over a wall that spans an elevation change requires
  the sill to match the intended walking surface.
* **Collision follows the solid slices.** Every opening removes exactly its
  rectangle from the wall solid; headers never block a walking player, and a sill
  above foot height blocks. There is no separate collision toggle.
* **Lighting transmits pools through every kind of hole**, but only `door`/`passage`
  openings whose bottom reaches the lower floor blend baselines; `window` and `vent`
  never do.
* **Openings are clamped to the wall footprint and the local ceiling.** An opening
  larger than the wall can remove it entirely; an opening whose vertical span misses
  the wall entirely is accepted by validation and silently does nothing.
* **Rejections** name the problem: `Wall {i} opening {j} starts before the wall`,
  `… cannot have a negative sill height`, `… must have a positive width and height`,
  and `Door/Window opening extends beyond this wall (wall {i}: opening ends at {x} m,
  wall is {y} m long)`. A wall with more than 64 openings is rejected
  (`Wall {i} has too many openings: …`).
* **Doorway thresholds**: adjacent rooms meeting at a doorway should have their
  floors meet at the wall's centre plane. The renderer subtracts floor coverage from
  wall caps so the two floors jointly cover the threshold exactly once. Do **not**
  author a sill-top surface that is coplanar with a floor; express a raised
  threshold as a floor region instead.
* **Windows/vents do not connect baselines**, so a room lit only through a window
  stays at its own baseline + the pool that physically passes through the aperture.

---

## 10. Floors, Elevation and Vertical Geometry

### `floor_patches` — material-only overlays

```json
{ "x": 3.0, "z": 5.0, "width": 2.0, "depth": 1.5, "material": "core:carpet_damp_01" }
```

`material` and the geometry are required; `shine` is an optional per-patch
glossiness override (`0`–`1`). A patch changes the floor material of an area with no
elevation change. Later patches win over earlier ones, and a floor region's own
material wins over patches. Patches are counted against the 2000-patch cap, but
individual patches are **not dimension-validated**: malformed values are skipped at
build time and a patch outside a room simply covers nothing. Keep them inside a room
and well-formed.

### `floor_regions` — recesses and raised platforms

```json
{ "x": 2.0, "z": 2.0, "width": 4.0, "depth": 3.0,
  "offset_y": -1.5,
  "material": "core:pool_tile_basin_01",
  "edge_material": "core:pool_tile_wall_01" }
```

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Minimum corner. |
| `width`, `depth` | number | **yes** | — | `> 0`. |
| `offset_y` | number | no | `0.0` | Offset from the **containing room's `floor_y`**: negative recesses, positive raises. |
| `material` | string | no | room floor material | Region floor material; if present it must be a non-empty id. |
| `shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for `material`. |
| `edge_material` | string | no | `defaults.wall` | Vertical transition (skirt) material; if present it must be a non-empty id. |
| `edge_shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for `edge_material`. |

Rules that matter:

* Regions are **not scoped to a room**: the same rectangle applies to every room it
  overlaps, resolved against that room's `floor_y`. The region's surface must stay
  below every overlapping room's eave or the level is rejected
  (`Floor region {i} sits at or above the ceiling of room {r}`), and a region that
  overlaps no room is rejected (`Floor region {i} lies outside every room section`).
* **Last authored region wins** per point, like patches. Overlapping regions are
  legal; they create their own skirts at their edges.
* Regions generate real vertical transition faces (skirts). Always give recesses an
  `edge_material` — a missing one falls back to the wall default and can look like
  unfinished space.
* `offset_y: 0` is a legal region that only changes material (like a patch) and
  emits no skirt.

### The walkable step rule

**A height change of at most 0.4 m is walked instantly; a larger change is a solid
rim** — solid from the lower side, and refused from the upper side. Staircases are
chains of floor regions whose consecutive offsets differ by ≤ 0.4 m (Places Demo
stair: 1.5 → 1.2 → 0.9 → 0.6 → 0.3 risers). This is what makes pool basins safe
without fall physics. The rim's blocking face sits on the region boundary; the
collider is a thin box extending 0.4 m under the higher floor so a sub-stepped move
cannot tunnel through it.

A rim **only ever blocks a change the player could not otherwise take**: it carries
the walkable step as headroom, so a player already within 0.4 m of the rim's top
(on a ramp or a staircase arriving beside the platform) walks past it. Its height
is sampled in short segments along the boundary, so a slope beside a rim is read at
its real local height instead of the cell's average. A rim never walls a landing
off.

Places Demo: the lowered pool basin (room `floor_y` is −1.5):

```json
{ "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0, "offset_y": -1.5,
  "material": "core:pool_tile_basin_01",
  "edge_material": "core:pool_tile_wall_01" }
```

The same room's walk-in step, one 0.35 m rise above the basin floor:

```json
{ "x": 10.0, "z": 16.0, "width": 6.0, "depth": 0.9, "offset_y": -0.35,
  "material": "core:pool_tile_basin_01",
  "edge_material": "core:pool_tile_wall_01" }
```

### Lowered rooms, raised rooms, pools

* **Whole-room elevation** uses `floor_y`; everything in the room moves together.
* **Partial elevation** uses `floor_regions`; a recess is a negative `offset_y`, a
  platform is positive. A pool basin is a deep negative region in a room whose floor
  is already lowered.
* **Transitions** are either ≤ 0.4 m steps (walkable) or solid rims. There are no
  ramps or sloped regions.
* **Stacked/vertically overlapping rooms are supported geometrically** (different
  `floor_y` over the same footprint) and are sealed from each other for lighting,
  but there is **no vertical traversal**: no stairs between storeys beyond the 0.4 m
  step rule, and **no floor/ceiling openings** exist as authorable features. Do not
  promise a multi-storey building; use one continuous level with room-to-room
  elevation steps, as Places Demo does (office 0.0 → stair hall −1.5 → corridor −0.9).
* **Light authors a storey**: when rooms share a footprint, an authored fixture `y`
  selects the storey (see [Rooms and lighting](#rooms-and-lighting)).
* **Lightmaps and floors are tessellated**: floors and ceilings are cut into
  roughly 2.5 m light grid cells (capped at 12 cells per axis) plus one cut line per
  patch/region edge, so materials and baked light can vary across a large room. This
  is automatic; there is nothing to author.

### Ramps and staircases

A level can author **sloped walking surfaces** (`ramps`) and **stepped walking
surfaces** (`stairs`) beside its rooms, walls and floor regions. Both are floor
surfaces, not props: the player walks them, the bake lights them, collision
answers with their real height, and they draw with the level's own materials.

```json
"ramps": [
  { "x": 4.0, "z": 0.4, "width": 1.0, "depth": 1.6,
    "offset_y": 0.0, "rise": 0.75,
    "material": "home:hardwood_oak_01",
    "edge_material": "home:wall_paint_offwhite_01" }
],
"stairs": [
  { "x": 2.2, "z": 2.6, "width": 1.4, "depth": 1.2,
    "offset_y": 0.0, "rise": 0.75, "steps": 5,
    "material": "home:hardwood_oak_01",
    "riser_material": "home:wall_paint_offwhite_01",
    "side_material": "home:baseboard_white_01" }
]
```

**Ramps** (`ramps[]`):

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Minimum corner of the footprint, like a wall or region. |
| `width`, `depth` | number | **yes** | — | `> 0`. The run follows the longer axis (ties → X), exactly like a wall. |
| `offset_y` | number | no | `0.0` | Surface offset at the **minimum-corner end**, relative to the room's `floor_y`. |
| `rise` | number | **yes** | — | Signed height change to the far (maximum-coordinate) end, in metres. `+1.0` climbs toward it, `-1.0` descends toward it. Not zero, at most 50 m, and at most `2.0 m` of rise per metre of run (a steeper slope is not walkable). |
| `material`, `shine` | string / number | no | room floor | The ramp's top surface. |
| `edge_material`, `edge_shine` | string / number | no | level wall default | The two closed side faces. |

**Staircases** (`stairs[]`):

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `x`, `z`, `width`, `depth` | number | **yes** | — | Footprint; the flight climbs along the longer axis (ties → X). |
| `offset_y` | number | no | `0.0` | Walking-surface offset at the **foot** of the flight. |
| `rise` | number | **yes** | — | Total climb, `> 0`, at most 50 m. |
| `steps` | integer | **yes** | — | Risers *and* treads; at least 2. The riser is `rise / steps` (must be ≤ 0.4 m, the walkable step) and the tread is `length / steps` (must be ≥ 0.15 m). |
| `material`, `shine` | string / number | no | room floor | The treads. |
| `riser_material`, `riser_shine` | string / number | no | the tread material | The risers. |
| `side_material`, `side_shine` | string / number | no | the riser material | The closed stringer sides. |

How the two behave:

* **They are walking surfaces.** A ramp's height is linear along its run and a
  staircase's is one riser per tread, both measured from the containing room's
  `floor_y`. The player controller steps or slopes over them with the ordinary
  0.4 m rule, and `PLACES_CAPTURE`-style inspection shows exactly what the
  player stands on.
* **The step rule runs per movement sub-step**, and a sub-step is at most 0.15 m,
  so the full legal range is walkable at every supported frame rate: at the
  steepest legal ramp (2 m of rise per metre of run) a sub-step rises at most
  0.3 m, and an exact-limit riser (0.4 m) is climbed even on a floor whose
  height is metres above zero.
* **They have no collision boxes of their own.** The walking surface *is* the
  collision: a step of more than 0.4 m at the piece's edge refuses the player,
  exactly like a floor-region rim. A ramp or flight that lands flush on a raised
  platform is closed by that platform's own skirt, and the platform's rim does
  not wall the landing off (the rim rule samples the walking surface on either
  side of a grid edge).
* **Their sides are real skirts.** A ramp's two sides drop from the sloped edge
  to the lowest floor they meet, and a staircase's sides close each step down to
  the floor line beside it, so a flight or slope is never an open wedge. The side
  is not a collider: the height rule keeps the player off it because the walkable
  floor inside the footprint is the piece's own surface.
* **The space under a ramp or flight is not walkable.** The walkable floor at a
  point over the footprint is the piece's own height, so the volume beneath it
  is solid to the player by construction.
* **They may not overlap a floor region or each other.** The loader rejects a
  region inside a ramp or staircase, a staircase inside a ramp, and a ramp
  inside a staircase: a space has one walking surface, and two floors in the
  same place would fight. They *do* meet a floor region edge-to-edge, which is
  how a flight lands on a platform.
* **They stay inside one floor plane.** A ramp or flight drawn across rooms with
  different `floor_y` values is rejected: its geometry is generated once, from
  the room under its centre, while the walkable surface would resolve each room's
  own floor. Rooms that share a floor plane are fine.
* **Their ends are closed against what they meet.** The low end of a ramp (and
  the foot of a flight) is level with the room floor, so no face is drawn there;
  an end that stands above the floor beyond it gets a real end face. The top
  lands flush on the platform, whose skirt closes it.
* **Validation**: non-finite or non-positive dimensions, a flat ramp, an
  over-steep ramp, an unclimbable riser, a too-shallow tread, a piece that
  overlaps no room, a piece that rises through the ceiling, or a piece crossing
  rooms with different floors are named errors.

### Half walls, columns, archways, guardrails, thresholds and baseboards

Six more arrays cover the ordinary architectural furniture of an interior. Every
one is theme-independent: it takes ordinary material ids, so the same geometry
is a Home skirting, an office partition or an industrial kick plate.

```json
"half_walls": [
  { "x": 6.05, "z": 6.6, "width": 1.0, "depth": 0.2, "height": 1.05,
    "material": "home:wall_paint_offwhite_01",
    "cap_material": "home:baseboard_white_01" }
],
"columns": [
  { "x": 3.7, "z": 2.1, "width": 0.26, "depth": 0.26,
    "material": "home:wall_paint_offwhite_01" }
],
"archways": [
  { "x": 5.83, "z": 1.6, "width": 0.34, "depth": 1.4,
    "height": 3.0, "opening_width": 1.0, "opening_height": 2.1, "arch_rise": 0.25,
    "material": "home:wallpaper_offwhite_01",
    "reveal_material": "home:wall_paint_offwhite_01" }
],
"guardrails": [
  { "x": 5.35, "z": 2.1, "length": 2.2, "rotation_degrees": 270.0,
    "height": 0.95, "material": "home:handrail_wood_01" }
  // beside a flight or ramp, omit `rise` and the rail follows the floor;
  // author `rise` (with `y`) to pin an explicit slope
],
"thresholds": [
  { "x": 6.0, "z": 2.3, "length": 1.04, "thickness": 0.08, "height": 0.012,
    "rotation_degrees": 90.0, "material": "home:threshold_wood_01" }
],
"baseboards": [
  { "x": 0.15, "z": 0.15, "length": 5.85, "height": 0.09, "thickness": 0.018,
    "material": "home:baseboard_wood_01" }   // back plane on the wall face
]
```

**Placement and the base height.** Half walls, columns and archways are placed
by **minimum corner** (`x`, `z`) like a wall; guardrails and baseboards by the
**start point of their run** (`x`, `z` = where the run begins); a threshold by
its **centre**. Every one of them takes an optional absolute world `y`, and
omitting it resolves the walkable floor under the piece (the footprint centre
for a box, the run's start for a rail or board). A piece authored `y` is
absolute, like a wall's — not floor-relative like a prop's.

**Orientation.** Guardrails, thresholds and baseboards run along their own local
`+X` axis and are rotated about Y by `rotation_degrees`: `0` runs east (`+X`),
`90` north (`−Z`), `180` west, `270` south. Half walls, columns and archways
derive their length axis from `width`/`depth` exactly like a wall (the longer
dimension; ties → X), and an archway's opening is centred on that length.

| Piece | Key fields | Materials | Collision |
| --- | --- | --- | --- |
| `half_walls[]` | `width`, `depth`, **`height` (required)** | `material` (length faces), `end_material`, `cap_material` | **Solid.** A knee wall, parapet or partition: it blocks and it occludes baked light. |
| `columns[]` | `width`, `depth`, optional `height` (default: floor to the local clear ceiling) | `material` (body), `cap_material` | **Solid.** A full-height column skips its cap where it meets the ceiling, so it never fights the ceiling plane. |
| `archways[]` | `width`, `depth`, `height` (block), `opening_width`, `opening_height` (at the crown), `arch_rise` (`0` = flat lintel) | `material` (faces and ends), `reveal_material` (jambs and soffit) | **Solid piers and spandrel, open doorway.** Collision covers the two piers and the wall above the opening only, so the opening is never blocked. The arch itself is eight flat segments. |
| `guardrails[]` | `length`, `height` (default 1.0), `rise` (slopes the rail; omitted follows the walkable floor), `post_spacing` (default 1.2) | `material` (rails), `post_material` (default: the rails') | **Solid barrier.** Its box spans the run from just below the base line to the top rail, so it stops the player from either side. Rail width and post section are fixed (0.07 m rail, 0.06 m post). |
| `thresholds[]` | `length`, `thickness` (default 0.06), `height` (default 0.012) | `material` (default: the level's floor) | **No collision.** A 12 mm strip of trim; the player walks over it. The loader rejects a strip whose ends stand at different floor heights (more than 0.05 m), that lies outside every room, or that is buried in a wall solid. |
| `baseboards[]` | `length`, `height` (default 0.09), `thickness` (default 0.018) | `material` (default: the level's wall) | **No collision.** The back face is not drawn (it is buried in the wall), and the run stands proud of the wall plane, so it never shares a plane with it. The loader rejects a board whose whole cross-section is inside a wall solid. |

Rules that matter:

* **Half wall, column and archway heights are rigid.** An authored height is
  drawn exactly, like a wall's authored height; a piece that is taller than the
  ceiling pokes through it. Columns default to the local clear ceiling and skip
  their cap when they meet it exactly (within 2 cm).
* **An archway needs piers.** `opening_width` must leave at least 0.08 m of
  block on each side, and the block must be at least as tall as its opening.
  Place the block so its ends tuck about 10 cm into the walls it interrupts:
  the block is slightly thicker than the wall is a comfortable way to case the
  opening, and its end caps then sit inside the adjoining wall rather than on
  its face.
* **Corners are solved by the engine, not by gaps.** Place each board's back
  plane on the wall's face and stop each end at the corner joint (the line where
  the two wall faces meet). Where two boards meet at a corner, the later-authored
  run gives up the overlap: its cap is trimmed against the earlier run's cap and
  its front face stops at the earlier run's face, so the two never share a
  coplanar surface and no gap is left. An end that is not a corner is closed with
  its own end face; an end buried inside a wall keeps its face hidden.
* **Boards belong on a wall face.** The room boundary is the wall's *centre*
  plane, so `"z": 0` against a 0.3 m wall puts the board 15 cm inside it. The
  loader rejects a board whose whole cross-section lies inside a wall solid, with
  the wall named. Use the wall's inner face (`z: 0.15` for a wall spanning
  `-0.15 … 0.15`).
* **Thresholds belong on a level floor.** Put one in a doorway between two
  floors at the same height (`length` a couple of centimetres wider than the
  opening tucks its ends into the jambs). A transition across a real step is a
  floor region or a small ramp, not a threshold strip.
* **Baseboards do not change the room's walkable surface** and are ignored by
  the lighting bake's occlusion: they are decoration with zero gameplay effect.
* **A guardrail is a barrier, and its posts are trimmed to fit.** A run that
  does not divide evenly by `post_spacing` gets an end post too; the rail and
  the posts use the same material unless `post_material` overrides it.
* **A guardrail is also a handrail.** With `y` and `rise` omitted the rail's base
  line follows the walkable floor from the run's start to its end, so a run
  placed from a flight's first nosing to its last nosing keeps a constant height
  above the treads; the same is true beside a ramp. Author `rise` (with `y`) to
  pin an explicit line, such as a level landing rail on sloping ground. Posts
  stay vertical and the barrier box follows the slope.

---

## 11. Materials

A level names a **material**, never a file path. The full resolution chain is:

```text
level JSON
  └─ material logical id        e.g. "core:wallpaper_yellow_01"
       └─ catalog material entry   (asset_type: "material", source: "definition")
            └─ texture logical id     e.g. "core:tex_wallpaper_yellow_01"
                 └─ catalog texture entry (asset_type: "texture", source: "file")
                      └─ PNG file     assets/environment/office/textures/walls/wallpaper_yellow_01.png
```

Materials are **definitions**, not files. A material entry may carry the fields below.
All of them are validated when the catalog is parsed; a bad value rejects the whole
catalog (with the asset id in the message), not just the field.

| Field | Type | Default | Range / rule | Behaviour when omitted |
| --- | --- | --- | --- | --- |
| `texture` | string | — | **Required** for a `material`. A logical `texture` id that must resolve to a file-backed `.png`. Only a `material` may declare it. | A material without it is a catalog error. |
| `tile_metres` | number | `2.0` | `0.05`–`64`. World metres covered by one repeat, both directions. Only a material may declare it. | Uses `2.0`, the historical sheet size. |
| `tint` | `[r,g,b]` | `[1,1,1]` | Each channel `0.0`–`1.0`. Static multiply on the sampled texture. Only a material may declare it. | White; no tint. |
| `surface` | string | none | `wall`, `floor` or `ceiling`. Documentation/validation only; geometry decides which family a material draws on, so any material may legally be used on any surface. | No surface tag. |
| `emissive` | `[r,g,b]` | none | Each channel `0.0`–`1.0`. Adds an emissive term on top of baked light. Only a `material` `definition` may declare emission. | The surface does not emit. |
| `emissive_intensity` | number | `1.0` when `emissive` is set | `0.0`–`8.0`. Multiplier on the emissive colour. Asserting it without `emissive` is a catalog error. | `1.0`. |
| `emissive_mask` | string | none | Logical id of a file-backed `texture`. Its RGB modulates where the surface emits; it must resolve or the whole material degrades to the diagnostic texture. Asserting it without `emissive` is a catalog error. | No mask; the material's own texture modulates the glow. |
| `normal_texture` | string | none | Logical id of a file-backed `texture` holding a tangent-space normal map (RGB = x/y/z encoded `0..255 → -1..1`). Must resolve or the whole material degrades. | No normal perturbation. |
| `normal_strength` | number | `1.0` | `0.0`–`2.0`. Multiplies the decoded map's `xy`. Asserting it without `normal_texture` is a catalog error. | `1.0`. |
| `specular` | number | `0.0` | `0.0`–`1.0`. Sheen strength: how much light the surface catches. `0.0` is the default flat look, and no `shine` value can switch a sheen on. | No sheen. |
| `specular_color` | `[r,g,b]` | white | Each channel `0.0`–`1.0`. Sheen colour; it does **not** require `specular`. With `specular: 0` the whole sheen term is zero, so the colour has no visible effect. | White sheen. |
| `shine` | number | `0.4` | `0.0`–`1.0`. Glossiness: `0.0` matte, `0.25` slight sheen, `0.5` semi-gloss, `0.75` polished, `1.0` extremely glossy. Shapes both the sheen and the reflection; a material with `specular: 0` never sheens at any shine. **Not a mirror** — a mirror is `reflection_mode: planar`. | The legacy `roughness` if authored, else `0.4`. |
| `roughness` | number | — | Legacy inverse of `shine` (`roughness = 1 - shine`), kept so catalogs authored before `shine` existed load unchanged. Author **either** field, not both; authoring both is a catalog error. | Prefer `shine`. |
| `alpha_mode` | string | `opaque` (absent) | `opaque`, `cutout` or `blend`. An unknown value is a catalog error. | `opaque`. |
| `opacity` | number | `1.0` | `0.0`–`1.0`. Multiplies the sampled alpha. Requires an explicit `alpha_mode`; only changes the image for `blend`, and shifts the threshold for `cutout`. | `1.0`. |
| `alpha_cutoff` | number | `0.5` | `0.0`–`1.0`. Alpha below which a texel is discarded. Requires an explicit `alpha_mode`; only `cutout` uses it. | `0.5`. |
| `reflection_mode` | string | `none` (absent) | `none`, `probe` or `planar`. An unknown value is a catalog error. See [Selective reflections](#selective-reflections-which-surfaces-reflect). | No reflection. |
| `reflection_strength` | number | `0.45` | `0.0`–`1.0`. Weight of the reflected image. Asserting it without `reflection_mode` is a catalog error. | `0.45` when a mode is set. |

Emission, surface-response, alpha and reflection fields are only valid on a
`material` whose `source` is `definition`. A prop, texture, light or generated entry
that authors any of them is a catalog error, because the renderer would never read
them.

Where a level can name a material (all resolved at load):

* `defaults.wall` / `defaults.floor` / `defaults.ceiling`
* `rooms[].material` (floor) and `rooms[].ceiling_material`
* `walls[].material` and `walls[].faces.<face>`
* `floor_patches[].material`
* `floor_regions[].material` and `floor_regions[].edge_material`
* `walls[].openings[].glass` (the pane filling an aperture, see
  [Panes](#panes-glass-grilles-and-screens))
* every generic architectural piece: `ramps[].material` / `.edge_material`,
  `stairs[].material` / `.riser_material` / `.side_material`,
  `half_walls[].material` / `.end_material` / `.cap_material`,
  `columns[].material` / `.cap_material`, `archways[].material` /
  `.reveal_material`, `guardrails[].material` / `.post_material`,
  `thresholds[].material`, `baseboards[].material`

Every one of those carriers takes an optional per-surface `shine` override as a
sibling key, so a level can change how glossy **one surface** is without a new
material:

| Carrier | Per-surface shine key |
| --- | --- |
| `defaults.wall` / `defaults.floor` / `defaults.ceiling` | `wall_shine` / `floor_shine` / `ceiling_shine` |
| `rooms[].material` (floor) | `rooms[].shine` |
| `rooms[].ceiling_material` | `rooms[].ceiling_shine` |
| `walls[].material` (length faces) | `walls[].shine` |
| `walls[].faces.<face>` | `walls[].face_shine.<face>` |
| `floor_patches[].material` | `floor_patches[].shine` |
| `floor_regions[].material` / `edge_material` | `floor_regions[].shine` / `edge_shine` |
| `walls[].openings[].glass` | `walls[].openings[].glass_shine` |
| `ramps[].material` / `edge_material` | `ramps[].shine` / `edge_shine` |
| `stairs[].material` / `riser_material` / `side_material` | `stairs[].shine` / `riser_shine` / `side_shine` |
| `half_walls[].material` / `end_material` / `cap_material` | `half_walls[].shine` / `end_shine` / `cap_shine` |
| `columns[].material` / `cap_material` | `columns[].shine` / `cap_shine` |
| `archways[].material` / `reveal_material` | `archways[].shine` / `reveal_shine` |
| `guardrails[].material` / `post_material` | `guardrails[].shine` / `post_shine` |
| `thresholds[].material` | `thresholds[].shine` |
| `baseboards[].material` | `baseboards[].shine` |

```json
{ "x": 15.8, "z": 0.2, "width": 3.0, "depth": 2.8,
  "material": "core:linoleum_polished_01", "shine": 0.05 }
```

A carrier without an override keeps the material's own default. A malformed
override (outside `0.0..=1.0`, or not a number) is a level error with the
surface named, not a silent clamp. `shine` only moves a surface along the
glossiness range: it cannot give a `specular: 0` material a sheen and it can
never turn a surface into a mirror.

The renderer multiplies: **sampled texture × material tint × baked light**, where
*baked light* is the lightmap atlas texel for static world geometry (see
[Lighting](#18-lighting)) and the baked vertex colour on the vertex-lit fallback
path. There is no gamma handling; the shipped art, tints and lighting constants were
calibrated together in that space. Author with the tint in mind (the office wallpaper
tint, for example, is `[0.85, 0.80, 0.42]`, so the PNG is authored pale).

### Emission: materials that glow

Emission is a **material** property and nothing else:

```text
how bright a surface reads        = material emission   (emissive x emissive_intensity x mask x texture)
how much a room is illuminated    = generic light sources (see section 18)
```

The two are independent by construction. An emissive surface is added *on top of* the
baked lighting, so it stays visibly bright in a dark room, and it never brightens
anything around it: **emissive materials do not cast light.** Author an ordinary
fixture (`ceiling_lights`) or a prop-attached light (`props[].lights`) if the object
should also illuminate the room.

* **No mask**: emission is modulated by the material's own texture, so artwork shapes
  the glow (`emission = emissive x intensity x texture`).
* **With `emissive_mask`**: the mask's RGB also modulates it, restricting the glow to
  the masked region.
* **Old materials**: a material without `emissive` emits nothing and renders exactly
  as it always did.
* **Fixture faces** are emission too: a placed fixture's visible face glows with its
  authored `color` and `emission` (which defaults to its `brightness`) and never takes
  part in the room's baked light. See [Light Placement](#21-light-placement).

Failure behavior: an unknown material id, a non-material id, a dangling texture, or a
mask/normal map that cannot resolve logs a `[materials] {level}: …` line and draws the
shared magenta/black diagnostic texture with **no** emission, response, alpha or
reflection (the whole material degrades; a half-resolved material would be worse than
an obvious placeholder).

Never write a filesystem path where a logical id is expected. A path in `material`
resolves to nothing and draws the diagnostic pattern.

### Surface response: shine, specular and normal

The surface-response set is the smallest set of numbers that makes two surfaces read
differently under the *existing* baked light. It is **not** a physically based model:
there is no realtime light direction in the bake, so there is no highlighted specular
to place, and nothing here samples the framebuffer.

```text
what a surface draws = texture x tint x baked light     (the historical term)
                     + sheen                            (specular x Fresnel x baked light)
                     + reflection                       (a marked surface's probe or plane)
                     + emission                         (the material's own brightness)
```

* **Sheen** (`specular`, `specular_color`) is *view dependent*: a surface catches
  more of the room's light as it turns away from the camera. `specular` is the
  material's identity — how much light the surface can catch at all — and
  `specular_color` tints it (a metal catches its own cool colour). It is scaled
  by the baked light, so a glossy surface in an unlit room stays dark.
* **Shine** (`shine`, `0.0`–`1.0`) is how glossy the surface is. It shapes the
  sheen *and* any reflection: a low-shine surface keeps only a broad, weak
  grazing sheen, and a reflection on it is dim and reads a wide average of the
  room; a high-shine surface gets a tight highlight and a sharp, recognizable
  image. A material with `specular: 0` never sheens or reflects at any shine.
* **Normal map** (`normal_texture`, `normal_strength`) perturbs the shading
  normal per texel. The tangent frame comes from the geometry's own UVs, so the
  map is oriented with the surface's tiling and a mirrored UV layout flips it
  correctly. Every mesh a level builds carries a geometric frame; nothing is
  authored per vertex.

The places' art direction is deliberately dull: **reflections should be subtle
enough that the player notices them only when looking for them**, with mirrors
and intentionally polished surfaces as the exceptions. Author the default of an
ordinary room surface near matte.

What to author for the usual cases:

| Look | `specular` | `shine` | Notes |
| --- | --- | --- | --- |
| Wallpaper / painted wall / ceiling / carpet | `0.0` | anything | no sheen at all; the default |
| Unfinished wood, bare concrete | `0.0`–`0.15` | `0.0`–`0.1` | practically matte |
| Institutional linoleum / vinyl | `0.2`–`0.35` | `0.0`–`0.1` | ordinary floors; not a waxed finish |
| Varnished wood, satin plastic | `0.25`–`0.4` | `0.3`–`0.45` | a visible but restrained sheen |
| Glazed tile (pool areas) | `0.2`–`0.3` | `0.25`–`0.4` | a low sheen, never a mirror |
| Painted metal, rough/aged metal | `0.4`–`0.6` | `0.2`–`0.35` | broad highlights and some environment colour |
| Brushed/stainless metal fixture | `0.5`–`0.65` | `0.4`–`0.6` | visibly metallic, softer than polished |
| Deliberately waxed floor / polished metal | `0.4`–`0.6` | `0.6`–`0.85` | the shiny end of ordinary materials |
| Wet surface | `0.5`–`0.65` | `0.6`–`0.8` | a `floor_patches` entry over the dry material |
| Mirror | `0.8`–`1.0` | `0.9`–`1.0` | **and** `reflection_mode: planar`; shine alone is not a mirror |

The same `shine` range applies to one material reused at different glossiness:

```json
{ "material": "core:metal_brushed_01" }                          // aged: the material default
{ "material": "core:metal_brushed_01", "shine": 0.1 }            // dull, still reads as metal
{ "material": "core:metal_brushed_01", "shine": 0.7 }            // a deliberately polished fixture
```

Material identity stays separate from shine: the sheen colour, the normal map
and the reflection mode keep a metal reading as metal at every shine value, and
a shiny linoleum floor never becomes polished steel.

A material that authors none of these fields adds nothing to the pixel: no
normal map, no sheen and `alpha_mode: opaque` are the defaults.
`tools/assets/validate.py` rejects out-of-range values by name, and a normal
map that cannot resolve degrades the whole material to the diagnostic texture
(it does not silently flat-shade).

**Quality profiles.** `Full` draws the response; `Low` leaves the normal-map and
sheen terms out and keeps albedo × light × emission × alpha. Both profiles use
the same materials and the same PNGs — the difference is one shader gate, not a
second art set.

**Art direction.** A normal map on this renderer is *detail on a flat surface*,
not a substitute for geometry: it cannot cast a shadow, it does not change the
silhouette and it is lit only by the baked room light. Keep the low-poly
vocabulary — bumps, grime, brushed streaks and panel seams, not sculpted detail.

### Selective reflections: which surfaces reflect

Two authorable paths can show a surface the room back: a **static probe** (a small
cubemap baked once per level load) and a **planar mirror** (a real second view of the
level through the surface's own plane). Neither is a screen-space effect, and
**nothing reflects unless a material asks** via `reflection_mode`. Shine and
reflection are separate: an extremely glossy ordinary material (`shine: 1.0`)
without a `reflection_mode` sheens but never samples the room, and a mirror is
always the dedicated `planar` behaviour rather than a shine value.

```json
{ "id": "core:pool_deck_wet_01", "asset_type": "material", "source": "definition",
  "texture": "core:tex_pool_tile_deck_01", "tile_metres": 1.5,
  "specular": 0.55, "shine": 0.72,
  "reflection_mode": "planar", "reflection_strength": 0.3 }
```

| `reflection_mode` | What it draws | Cost |
| --- | --- | --- |
| `none` (default) | nothing | none |
| `probe` | the static cubemap baked at level load, read by the reflected view vector | one texture read per reflective fragment |
| `planar` | a real second view of the level, mirrored through the surface's plane | one extra scene pass per frame while that plane is on screen |

Four properties are worth designing around:

* **It rides on the sheen and the shine.** The reflected colour is weighted by
  the material's own `specular` colour, its `shine` and the view angle. A
  material with `specular: 0` never reflects. At low shine the reflection only
  appears near grazing angles, at reduced weight, and reads a broad average of
  the room rather than a recognizable image; a highly polished surface reflects
  across the whole face. There is no separate "reflectivity" number to keep in
  step with the sheen, and `reflection_strength` is a weight on top (default
  `0.45`, maximum `1.0`).
* **A per-surface `shine` override also re-shapes the reflection** (and the
  sheen) on that surface alone, because both read the same value. It never
  changes *where* the reflection comes from, so a matte override on a marked
  surface keeps the (now faint) probe or planar reflection rather than removing
  it.
* **It is approximate.** A probe is a 64-texel-per-face cubemap (32 on `Low`) — the
  shape of the room, not a second render of it — and a planar reflection is drawn at
  half resolution. Use them where the surface should read as wet, polished or
  mirrored, not where the player will compare the reflection with the room.
* **A planar mirror shows the room through its own surface.** The mirror plane's
  geometry is left out of the mirrored draw, so the reflected image is what the
  mirrored camera sees through the plane rather than the plane's own colour.
  That works because a planar surface is an *aperture*: a floor or ceiling
  plane, a floor patch or region, or an opening's pane. A wall **slab** is not
  one — it emits several faces and its caps span its thickness, so the plane
  cannot be derived and the marking is skipped with a log line. Put a wall
  mirror on an opening as an opaque `glass` pane instead.
* **Probes are clustered, and there are at most two.** Reflective probe geometry is
  clustered by distance (a room-sized 12 m radius, area-weighted centroids), and only
  the two largest clusters get a probe; the nearest probe is sampled per fragment.
  A probe sits 1.2 m above the surface that asked for it.
* **Mark only where it is worthwhile.** Planar reflections are the expensive
  half: at most one plane is drawn per frame, chosen as the nearest one whose
  geometry is on screen. Marking several walls will make them take turns. Places
  Demo ships **one** planar surface (the wet pool deck) and two probe materials
  (polished linoleum and brushed metal).
* **The surface must be geometrically flat to work as a mirror.** The plane is
  derived from the emitted geometry at load (all of a material's vertices must lie on
  one plane); a material reused on a curved or stepped surface is reported
  (`[reflections] material {n} marks a planar reflection but its geometry is not
  planar; skipping that range`) and skipped rather than reflected wrongly. The
  material keeps its sheen and loses only the mirror image. In practice a planar
  material belongs on an axis-aligned floor, wall or ceiling pane; a probe is the
  right choice for anything else.

**Quality profiles.** `Full` draws the planar pass and 64-texel probes; `Low`
draws the probes at 32 texels and never allocates a planar target. A material
marked `planar` simply keeps its sheen and loses the mirror image on `Low`.

Shipped examples: `core:pool_deck_wet_01` (`planar`, 0.4, the wet deck patch),
`core:linoleum_polished_01` (`probe`, 0.25, the deliberately waxed end — Places
Demo overrides its ordinary linoleum patch down to `shine: 0.05`) and
`core:metal_brushed_01` (`probe`, 0.25, aged metal).

### Transparency: alpha modes

`alpha_mode` is a **material** property; a level never authors a render order.
The renderer decides which pass a surface lands in, and there are exactly three:

| `alpha_mode` | Behaviour | Pass |
| --- | --- | --- |
| `opaque` (default) | The texture's alpha channel is ignored entirely. | Opaque: depth writes on, blending off. |
| `cutout` | Texels below `alpha_cutoff` (after the `opacity` multiplier) are discarded; the rest are written opaque. | Opaque, through the alpha-tested fragment stage (a separate program, so the opaque pass keeps early depth testing). |
| `blend` | The texel's alpha (texture alpha × `opacity`) blends the surface over what is behind it. | Translucent: after everything opaque, sorted back to front per spatial batch, depth *testing* on and depth *writing* off. |

Consequences worth knowing:

* Two overlapping translucent surfaces blend correctly because the pass is sorted
  back to front by the distance from the camera to each batch. Sorting is per
  spatial batch (the granularity the renderer already partitions the world at),
  not per triangle.
* A translucent surface never occludes anything: a pane of glass is hidden by the
  wall it sits in, but does not hide the room behind it in the depth buffer.
* Emissive translucent materials work: emission is added to the lit term before
  the alpha blend, so a backlit sign is *both* bright and see-through. Author it
  with `emissive` plus `alpha_mode: blend` (Places Demo's
  `core:glass_sign_lit_01` is exactly that).
* Decals are unaffected: they are their own pass with a cut-out and a depth bias,
  authored as decal sheets (section 17), not as materials.
* A `blend` material with `opacity: 0` is invisible and is skipped entirely.
* Transparency is alpha blending, not refraction: nothing bends, and the lighting
  bake still treats the aperture as an open hole (see the glazing note below).
* GLB props are always drawn opaque: a prop's glTF `alphaMode` is not read. Only
  level surfaces and fixture faces have material alpha.

Authoring a transparent sheet is ordinary artwork: RGBA, with the alpha channel
carrying the coverage (a grime film, a tint, a cut-out pattern). `tile_metres`
applies as usual.

### Panes: glass, grilles and screens

An opening may carry a **pane**: a `glass` material that fills the aperture with
one surface at the wall's centre plane. It is what turns "a hole in a wall" into
"a window with glass in it".

```json
{ "kind": "window", "offset": 1.65, "width": 3.2, "height": 1.3, "sill": 1.7,
  "glass": "core:glass_window_dirty_01" }
```

* `glass` takes an **ordinary material id**, so the pane's tint, dirt, shine,
  sheen, emission and alpha mode are the material's, not the opening's. A
  `glass_shine` override can re-shine one pane without a second material.
* The pane is the opening's own rectangle: no frame, no thickness, one surface
  seen from both sides. It is **not lightmapped**: its four corners sample the
  baked light directly and fold it into the vertex colour, like a fixture face or a
  prop.
* Collision still follows the wall's solid slices (a raised window still blocks),
  and the lighting bake still transmits through the aperture as an open hole —
  glass does not darken the room behind it. Tint the glass to imply that in the
  artwork.
* Any alpha mode works. `blend` gives real glass; `cutout` gives a grille,
  mesh or screen with holes in it (`core:grille_vent_01` is a transfer grille,
  authored on a `vent` opening above Places Demo's office door); `opaque` gives a
  solid panel, which is also how to fill an aperture with a blanking plate.
* Author each physical wall once, as everywhere else: two coincident walls each
  emit their own pane.

Shipped glass materials: `core:glass_window_clear_01`, `core:glass_window_dirty_01`,
`core:glass_tinted_01`, `core:glass_sign_lit_01` (translucent + emissive),
`core:glass_sign_flicker_01` (the same sheet, meant for `animated_emissions`), and
`core:grille_vent_01` (cut-out).

### Post-processing, fog and grading are engine-global

Bloom, the tone shoulder, distance fog and the colour grade are **not level
properties**. They are built from the quality profile (`src/render/common/postprocess.rs`,
`src/render/common/atmosphere.rs`) and there is no level key, material field or room field
that authors them. Two consequences are still useful to a map author:

* **Bloom follows emission, not brightness.** Only a surface whose material (or
  fixture face) emits blooms; a brightly lit wall never does, because the bloom pass
  draws the emissive term alone. If a fixture should glow, author emission on it.
* **Fog is depth, not weather.** The shipped density gives about 4 % at 20 m,
  15 % at 40 m and 63 % at the 100 m far plane, a little denser near the floor. It is
  most visible down a long corridor, and it never turns a room smoky.
* `Full` draws the profile resolve (tone shoulder and grade); `Low` presents the
  scene unfiltered and keeps only the fog, which lives in the world shader.
* Bloom is a **player setting** (Settings → Graphics → Bloom, default on), not a
  profile term, so both profiles can bloom. The default `Low` presents unfiltered
  only when Bloom is off.

### Level packs: `materials.json`

A `.zip` pack may ship material definitions and textures. A pack material is named
with a `pack:` id (e.g. `pack:lobby_wall`); a pack **cannot shadow a catalog id**, and
a level references the pack id exactly like any other material id.

`materials.json` accepts either a top-level object of definitions or one nested under
`"materials"`. Each value is either a string (the texture path — the shorthand, which
is emission-free and has no response/alpha fields) or an object:

| Key | Meaning |
| --- | --- |
| `texture` (aliases `file`, `diffuse`) | Pack-relative path or a logical catalog texture id. Required on the object form. |
| `tile_metres`, `tint` | As in the catalog; malformed values are discarded and the default applies. |
| `emissive`, `emissive_intensity`, `emissive_mask` | As in the catalog; the mask may be a pack path or a catalog texture id. |
| `normal_texture`, `normal_strength` | As in the catalog; pack path or catalog texture id. |
| `specular`, `specular_color`, `shine` (and the legacy `roughness`) | As in the catalog; `shine` wins if both are present. |
| `alpha_mode`, `opacity`, `alpha_cutoff` | As in the catalog. |
| `reflection_mode`, `reflection_strength` | Accepted, but see the limitation below. |

A texture path inside the pack is looked up with tolerant aliasing
(`textures/<name>.png`, `<name>.png`, `textures/<name>`, `<name>`), so a pack may lay
its files out either way. Unknown keys are ignored and malformed values fall back to
defaults; malformed `materials.json` yields no definitions at all (the pack's
`pack:` ids then fall back to the direct-name lookup).

**Known limitation — reflection settings on pack artwork.** A pack material whose
opaque `texture` is a **logical catalog texture id** keeps its
`reflection_mode`/`reflection_strength`. A pack material that ships its **own PNG**
applies emission, sheen and alpha but currently drops the reflection settings
(`reflection_mode` has no effect). Plan reflective pack surfaces to reuse a catalog
texture, or accept the sheen without a reflection.

## 12. Textures

**Repository rule: normal editable game textures must exist as real image files in
the asset tree.** Do not generate normal game artwork procedurally from Rust/source
code at runtime. Create the PNG, register it in the catalog, reference it by logical
id. The few remaining code-generated images are internal diagnostics/UI and are
listed at the end of this section — they are exceptions, not the authoring path.

### Supported formats and limits

| Property | Value |
| --- | --- |
| File format | **PNG only.** Signature-checked; RGB, RGBA, grayscale, grayscale+alpha and palette (with/without `tRNS`) all normalise to 8-bit RGBA; 16-bit is stripped to 8-bit. |
| Hard edge limit | **1024 px** on either edge, enforced by the runtime decoder for every PNG (surfaces, decals, fixtures, pack textures, embedded GLB images): `texture dimensions {w}x{h} exceed the 1024x1024 limit`. |
| Preferred edge | **256 px** — a soft tooling/policy warning, not a runtime error. |
| Surface sheets | **Square** (both edges equal) per shipped-asset policy; the runtime accepts another shape, but a `tile_metres` cell would stretch. |
| Decal sheets | **Power-of-two on both edges** (mipmapped fitted sampling; POT is the shipped-asset policy). |
| Fixture faces | **Power-of-two on both edges** likewise. |
| Decoded surface budget | ≤ 4 MiB RGBA8 per sheet (one 1024×1024 sheet is exactly 4 MiB). |
| Surface wrapping | `REPEAT` + mipmaps. Surfaces are expected to tile. |
| Decal wrapping | `REPEAT` + mipmaps, but full-sheet fitted UVs, so the sheet never actually repeats. |
| Fixture wrapping | `CLAMP_TO_EDGE` + mipmaps; the whole sheet is fitted once across the face. |
| Alpha | Surface sheets are opaque *unless* their material authors `alpha_mode`. The base pass has blending off, so an `opaque` material's alpha channel is ignored; `cutout` discards texels below the material's cutoff and `blend` samples it. Decals are alpha cut-outs (alpha < 0.5 discarded). Fixture faces are opaque. |
| Normal maps | A normal map is an ordinary RGB sheet in the same asset tree; the material names it with `normal_texture`. It is a *surface* texture for quality purposes (Full 1024 / Low 256), tiles like its albedo and may be hand-painted or generated. |
| Colour space | No gamma handling; texture × tint × baked light in display space. |

Shipped Office/Pool surface sheets are intentionally 1024×1024, square, opaque;
`python3 tools/textures/build.py --check` reports them as "over preferred" warnings by
design. The preferred 256 px size is a budget warning, not a rejection.

### Runtime quality profiles and downscaling

The source PNG is *not* what necessarily reaches the GPU. Two quality profiles
(`settings.json` → `"quality": "full" | "low"`, default `full`) decide a **runtime**
edge budget per texture class. The profile is a selector in Settings → Graphics
and can be changed while a level is running: the renderer releases its
profile-dependent GPU textures and rebuilds them (plus the lightmap atlas) from
the level already resident, so the player, camera and game state are preserved:

| Texture class | Full (default) | Low |
| --- | --- | --- |
| Surface sheet | 1024 | 256 |
| Fixture face | 1024 | 256 |
| Decal sheet | 1024 | 256 |
| Prop sheet (GLB) | 256 | 128 |
| Emissive mask | 512 | 128 |
| Lightmap atlas page | 1024, 16 texels/m | 512, 9 texels/m |
| Shadow penumbra taps | 2 per axis (5) | 1 per axis (hard) |
| Prop occlusion grid | 0.075 m | 0.15 m |

* **Full is the native Places runtime size.** Every shipped asset is already at or
  below it, so Full uploads the decoded image unchanged — no rescaling, no visual
  change. A native 256×256 prop sheet stays 256×256.
* **Low uses the same assets** and box-filters each image once, at level load, to the
  Low budget. It is not a second art library; ids, materials and geometry are
  identical. Low is an optional quality/performance trade, never a repository asset
  requirement.
* Downscaling happens once per upload, never per frame, and the result is cached with
  the texture it produced. Full and Low are deterministic: the same source always
  produces the same runtime image.
* The source hard limit (1024 px) is unchanged by either profile: quality only decides
  how much of an accepted source reaches the GPU.
* The **lightmap atlas** is baked light data, not shipped artwork (see
  [Baked lightmaps](#baked-lightmaps)): the same level bakes at the profile's density,
  so Low needs no separate level or hand-authored lightmap set.
* Do **not** author a separate level or asset set for Low. Both profiles run the same
  level content.

An author does not need to do anything differently for Low: ship the sane source
size and let the engine fit it.

### Tiling, orientation and seams

* **Tileable** in both directions for surfaces: the right edge must join the left,
  the top the bottom. Tile seams are checked for the shipped environment set by
  `python3 tools/textures/seam_repair.py --check` and by
  `test_shipped_surface_textures_tile`; `tools/textures/build.py --check` does **not**
  verify tileability.
* **Wall orientation**: the image's top row is the top of the wall, and its left edge
  is on the viewer's left from the side the face looks into. A tile is `tile_metres`
  tall and the phase is anchored to the wall top.
* **Floor/ceiling orientation**: image `x` maps to world **+X**, image `y` maps to
  world **+Z** (an authored map-style image reads with north/−Z up).
* `tile_metres` is the world-space period for both axes.

### Naming and directories

```text
assets/environment/<theme>/textures/walls/<name>_01.png
assets/environment/<theme>/textures/floors/<name>_01.png
assets/environment/<theme>/textures/ceilings/<name>_01.png
assets/environment/<theme>/textures/lights/<name>.png     fixture faces
assets/environment/<theme>/decals/<name>_01.png           decal sheets
assets/core/decals/<name>_01.png                          shared decal sheets
assets/core/props/models/<name>.glb                       shared props
assets/entities/<id>/model/<id>.glb                       entities
assets/diagnostic/textures/*.png                          engine test artwork (do not ship)
```

Convention (not enforced): texture ids use a `tex_` prefix
(`core:tex_wallpaper_yellow_01`); the material drops it
(`core:wallpaper_yellow_01`); numbered variants end `_01`.

### What each kind of image is for

| Kind | What it is | Tiling | Alpha | Catalog type |
| --- | --- | --- | --- | --- |
| Repeating surface texture | Wall/floor/ceiling artwork | Tiles (`REPEAT`) | Per the material's `alpha_mode` | `texture` + a `material` |
| Normal map | Tangent-space detail for a material | Tiles (`REPEAT`) | Opaque (alpha unused) | `texture` + a material's `normal_texture` |
| Decal artwork | A sign/marking cut-out placed on a surface | Fitted once (sheet never repeats) | Alpha cut-out (background alpha 0) | `decal` |
| Fixture-face artwork | The visible lit face of a light fixture | Fitted once | Opaque | `light` |
| Model-embedded texture | A prop's texture, inside its GLB | Fitted per the model's UVs | Per model | inside the GLB, no catalog texture entry |

Do not create separate `texture` catalog entries for decal sheets or fixture faces —
their `model` field *is* the PNG. (A surface material still needs its own `texture`
entry.)

### Generated internal exceptions (not the authoring path)

| Resource | Where | Purpose |
| --- | --- | --- |
| Missing-texture diagnostic (64×64 magenta/black) | `src/materials/image.rs` | Visible fallback for any broken surface/decal/fixture texture. |
| Generated decal atlas (256×256; only `core:decal_test_01`) | `src/render/common/decals.rs` | Internal validation marking; the external decal sheets are ordinary PNGs. |
| White sheet (2×2) | `src/render/common/mod.rs` | Untextured geometry (fixture housings, UI quads). |
| HUD font atlas (128×64) | `src/font.rs` | Project-owned bitmap UI font. |
| Lightmap atlas (up to two pages, quality-profile sized) | `src/lighting/lightmap/` | Baked *light data*, derived at level load from the level's own lights and geometry — the texel equivalent of the baked vertex colours it replaces. Not authored artwork, and deliberately not shipped as PNGs: it changes whenever a light, prop or surface moves, and it is regenerated (never re-saved) on load. |

Everything else the renderer draws from an image comes from a PNG under `assets/`.
Every *surface, fixture, decal and prop texture* is still a real PNG asset under
`assets/`, including the lightmap's albedo partners; the lightmap atlas is the only
thing the renderer samples that is generated at runtime, and it is lighting data
rather than texture artwork.

### Texture budget summary

| Budget | Value | Enforced by |
| --- | --- | --- |
| PNG hard edge | 1024 | runtime decoder + `tools/textures/build.py` + package tests |
| Preferred edge (soft) | 256 | tooling warning + shipped-asset policy tests |
| Surface square | both edges equal | policy tests |
| Surface decoded bytes | 4 MiB | policy tests |
| Prop native edge | 256 | prop toolkit + props policy tests |
| Prop pack decoded memory | 64 MiB | `tools/props/build.py --check` + props policy tests |
| Decal / fixture faces | POT both edges | build.py warning; package tests fail non-POT fitted sheets |

---

## 13. Asset Organization and Themes

Asset classes in the current catalog:

| Class | In shipped data? | Contents |
| --- | --- | --- |
| `environment` | Yes (all theme + generic content) | Surfaces, props, decals, fixture faces — including the shared props that live under `assets/core/`. |
| `diagnostic` | Yes | `assets/diagnostic/` engine test textures and the generated decal `core:decal_test_01`. |
| `entity` | Yes (one entry) | `spooner-man`. |
| `core` | No shipped entry uses it | Supported and accepted (engine-level shared resources); the `assets/core/` **directory** holds generic props such as `core:couch`, but their catalog class is `environment`. Class is independent of directory. |

Three themes ship: **`office`**, **`pool`** and **`home`**, declared in `assets/catalog.json`'s
`themes` array. A theme is an organizational collection with a display name and a
description. Generic/shared assets omit `theme`.

**Themes organize; they never restrict.** No code path rejects an asset because a room
has a different theme, and there is deliberately no theme-filtering query. Place Pool
fixtures in an office, mix themes in one room, or use generic `core` props anywhere.
Rooms have no mandatory theme field.

### The Home theme

`home:` materials are ordinary catalog definitions, so a level uses them like any
other material id. The canonical set is **clean by design**: no dirt, stains,
water damage or wear, and the office set's stained/damp variants remain available
for a level that wants them.

| Material id | Surface | Intended use | Notes |
| --- | --- | --- | --- |
| `home:wallpaper_offwhite_01` | wall | The default clean residential wallpaper | Matte (`specular: 0.0`), no pattern; 2 m tile |
| `home:wallpaper_pattern_01` | wall | A second wallpaper with one very subtle repeating motif | Same paper, a 25 cm motif cell; still matte |
| `home:wall_paint_offwhite_01` | wall | Plain painted wall: distinct from wallpaper | A low satin response (`specular: 0.10`, `shine: 0.22`) |
| `home:hardwood_oak_01` | floor | Primary hardwood: a finished warm oak, 20 cm planks | `tile_metres: 1.6`, a restrained satin sheen |
| `home:hardwood_walnut_02` | floor | Secondary hardwood: darker walnut, 15 cm planks | A genuinely different floor, not a tint |
| `home:carpet_cream_01` | floor | Clean beige/cream carpet | Matte (`shine: 0.06`), no baked dirt |
| `home:tile_home_01` | floor | Kitchen / bathroom / utility tile | 15 cm tiles at a 1.2 m repeat |
| `home:ceiling_white_01` | ceiling | Flat white residential ceiling | Near-flat painted finish |
| `home:ceiling_plaster_01` | ceiling | Lightly textured plaster ceiling | A fine stipple, not a popcorn ceiling |
| `home:baseboard_wood_01` | wall | Wood skirting; also fine on rail trim | Fine horizontal grain; 0.5 m repeat |
| `home:baseboard_white_01` | wall | Painted white skirting | Smooth, faint brush grain |
| `home:handrail_wood_01` | wall | Handrail / guardrail timber | Varnished, a little glossier |
| `home:threshold_wood_01` | floor | Threshold strips at a floor-material change | Independent of the floors it joins |

The fixture is `home:ceiling_light_round` (see
[Current Light Fixture Types](#current-light-fixture-types)) and the two kitchen
cabinets are the `home:cabinet_base` / `home:cabinet_wall` props. Generic
`core:` props (`core:couch`, `core:bed`, `core:table`, `core:lamp`, `core:rug`,
`core:fridge`, `core:stove`, `core:sink`, `core:bookshelf`, `core:tv`,
`core:plant`) are already domestic and need no Home variant.

**Adding a future theme (e.g. `hotel`)** — no engine change:

1. Add a `themes` record:
   `{ "id": "hotel", "display_name": "Hotel", "description": "…" }`.
2. Create `assets/environment/hotel/…` with the textures/props/decals you need.
3. Register assets with `"theme": "hotel"`.
4. `tools/assets/validate.py` calls the theme known because it is declared; the Rust
   runtime never required it in the first place.

Theme ids are validated as lower-case slugs (`[a-z][a-z0-9_-]*`), like
`asset_class` and `asset_type`.

---

## 14. Asset Catalog

`assets/catalog.json` is the authoritative registry. Levels reference **logical ids**,
never paths; the catalog maps an id to its class, theme, type and resource.

```jsonc
{
  "format_version": 2,
  "themes": [
    // Each theme: { "id", "display_name", "description" }.
    { "id": "office", "display_name": "Office", "description": "…" }
  ],
  "assets": [
    // One entry per asset; see the field reference below.
  ]
}
```

Only `id`, `asset_class` and `asset_type` are required on an entry. Unknown JSON
fields are ignored; a JSON *type* error in a field like `size` or `tint` rejects the
whole document. `format_version` is parsed and currently ignored (no version check
anywhere). `display_name` has a legacy alias `name`.

### Entry field reference

| Field | Type | Requirement | Default / fallback | Applies to |
| --- | --- | --- | --- | --- |
| `id` | string | **required** | — | all. Non-empty; characters `[A-Za-z0-9:_.-]`, may not start with `:`. Duplicates are a catalog error. |
| `display_name` | string | optional | legacy `name`, then the id | all |
| `asset_class` | string | **required** | — | all. Validated lower-case slug; shipped: `environment`, `entity`, `core`, `diagnostic`. Unknown classes parse in Rust but fail `validate.py`. |
| `theme` | string | optional | none (generic) | all. Organizational only; must be a declared theme to satisfy `validate.py`. |
| `asset_type` | string | **required** | — | all. Shipped: `prop`, `entity`, `material`, `texture`, `light`, `decal`. |
| `source` | `"file"` \| `"definition"` \| `"generated"` | optional | inferred: `texture` → `definition`; else `model` → `file`; else `generated` | all. `file` requires a `model`; `generated` must not declare one; `definition` requires a `texture` and must not declare a `model`. |
| `model` | string | type-specific | — | Resource path **relative to `assets/`**. `.glb` for props/entities; `.png` for file textures, file decals and file lights. Rejected if absolute, backslashed or containing `..`/empty components. |
| `size` | `[w,h,d]` | optional (validator requires it for placeables) | invalid values dropped; runtime fallback `[0.6, 0.9, 0.6]` | props, entities. Used by editor/placeholder boxes; a level's own `size` overrides it for collision. |
| `color` | `"#rrggbb"` | optional | `#8a8a8a` | props, entities (placeholder box + editor) |
| `category` | string | optional | `"Other"` | props, entities (organizational) |
| `solid` | boolean | optional | `false` | props, entities (catalog advisory; level `solid` controls collision) |
| `surface` | string | optional | none | materials/textures; `wall`/`floor`/`ceiling` documentation/validation |
| `texture` | string | **required for `material`** | — | materials. Logical id of a `texture` asset. Not allowed on other types. |
| `tile_metres` | number | optional | `2.0` (`0.05`–`64`) | materials only |
| `tint` | `[r,g,b]` | optional | `[1,1,1]` (channels `0`–`1`) | materials only |
| `emissive` | `[r,g,b]` | optional | — | materials (`0`–`1` each; the anchor of the emission group) |
| `emissive_intensity` | number | optional | `1.0` | materials (`0`–`8`; requires `emissive`) |
| `emissive_mask` | string | optional | — | materials (logical file-backed texture id; requires `emissive`) |
| `normal_texture` | string | optional | — | materials. Logical texture id of a tangent-space normal map. |
| `normal_strength` | number | optional | `1.0` | materials (`0`–`2`; requires `normal_texture`) |
| `specular` | number | optional | `0.0` | materials (`0`–`1`): sheen strength |
| `specular_color` | `[r,g,b]` | optional | white | materials (`0`–`1` each; does not require `specular`, and has no visible effect without it) |
| `shine` | number | optional | legacy `roughness`, else `0.4` | materials (`0`–`1`): `0` matte, `0.5` semi-gloss, `1` extremely glossy. Not a mirror. |
| `roughness` | number | optional | — | materials (`0`–`1`): legacy inverse of `shine`; author either one, never both |
| `alpha_mode` | string | optional | `opaque` | materials: `opaque` / `cutout` / `blend` |
| `opacity` | number | optional | `1.0` | materials (`0`–`1`; requires an explicit `alpha_mode`) |
| `alpha_cutoff` | number | optional | `0.5` | materials (`0`–`1`; requires an explicit `alpha_mode`; only `cutout` uses it) |
| `reflection_mode` | string | optional | `none` | materials: `none` / `probe` / `planar`. See [Selective reflections](#selective-reflections-which-surfaces-reflect) |
| `reflection_strength` | number | optional | `0.45` | materials (`0`–`1`; requires `reflection_mode`) |
| `entity_type` | string | optional | none | entities |
| `description` | string | optional | none | all |
| `tags` | array of strings | optional | `[]` | currently unused |

A material's `texture` must name a declared, file-backed `texture` asset
(`source: "file"`, `model` ending `.png`); `emissive_mask` and `normal_texture`
follow exactly the same rule. A material may be declared before the
texture it draws with, but never with a dangling reference: the catalog does a second
pass after all entries exist, and any failure rejects the catalog.

The loader also accepts the legacy `props` array (old flat registry) and merges it
with `assets`; new content should use `assets` only. A legacy `props` entry may omit
`asset_class`/`asset_type` (defaulted to `environment`/`prop`) and an entry with an
empty id is skipped.

### Field examples

The snippets below are schema-verified. `pool:tile_blue_01`, `pool:decal_slip_01`
and the `hotel:` ids in the recipes are illustrative placeholders, not shipped ids.

```json
{ "id": "pool:tex_tile_blue_01", "display_name": "Blue Pool Tile Texture",
  "asset_class": "environment", "theme": "pool", "asset_type": "texture",
  "source": "file", "surface": "floor",
  "model": "environment/pool/textures/floors/pool_tile_blue_01.png" }
```

```json
{ "id": "pool:tile_blue_01", "display_name": "Blue Pool Tile",
  "asset_class": "environment", "theme": "pool", "asset_type": "material",
  "source": "definition", "surface": "floor",
  "texture": "pool:tex_tile_blue_01", "tile_metres": 2.0 }
```

```json
{ "id": "pool:decal_slip_01", "display_name": "Slippery Floor Sign",
  "asset_class": "environment", "theme": "pool", "asset_type": "decal",
  "source": "file", "model": "environment/pool/decals/slip_01.png" }
```

```json
{ "id": "core:fluorescent_panel_01", "display_name": "Fluorescent Panel",
  "asset_class": "environment", "theme": "office", "asset_type": "light",
  "source": "file",
  "model": "environment/office/textures/lights/fluorescent_panel_01.png" }
```

### Catalog validation behavior

* Rust runtime (permissive): unknown classes/types parse; malformed `size` drops to
  the fallback; malformed `color` drops to the fallback; duplicate ids are
  **rejected**; a malformed material reference (missing texture, dangling texture,
  emission without colour, response field on a non-definition, out-of-range numeric)
  rejects the catalog and the whole catalog loads as empty.
* `python3 tools/assets/validate.py` (strict): rejects unknown classes/types/sources,
  missing files, duplicate model paths, bad `surface` values, missing built-in
  `office`/`pool` themes, and levels that reference undeclared ids. Always run it.

---

## 15. Supported Asset Types

One row per currently supported `asset_type`. This table is the extension point: when a
new asset type is added, add a row here and update the referenced sections.

| Asset type | Purpose | Physical resource | Placeable directly in a level? | Referenced by |
| --- | --- | --- | --- | --- |
| `prop` | Three-dimensional object | `model` = `.glb` under `assets/` | **Yes** — `props[].model` | levels, `prop_proxies.json` (derived) |
| `entity` | A special placeable actor (currently `spooner-man`) | `model` = `.glb` | **Yes** — same prop pipeline | levels |
| `material` | Surface appearance definition | `source: "definition"`, no file; names a `texture` | No | `defaults`, rooms, walls/`faces`, patches, regions, opening `glass` |
| `texture` | A surface PNG | `model` = `.png` | No | a `material`'s `texture`, `emissive_mask`, `normal_texture` |
| `light` | A fixture's visible face PNG (the fixture's mesh family is code) | `model` = `.png` | No (levels name it in `ceiling_lights[].fixture`) | level fixture ids; `src/lighting/tuning.rs` fixture table |
| `decal` | A surface marking sheet | `model` = `.png` (`source: "file"`), or `source: "generated"` for the internal test atlas | No (levels name it in `decals[].material`) | level decal placement |

`asset_class` is orthogonal to `asset_type` and `theme`. `generated` texture-like
assets exist only as internal diagnostics (see [Textures](#12-textures)).

---

## 16. Props and Models

Props and entities are **self-contained GLB files** placed by logical id. Textures are
embedded in the GLB; they are not separate catalog assets.

### GLB profile (what the runtime accepts)

* Container: binary **GLB, glTF 2.0**, JSON + BIN chunks.
* One **scene graph**: nodes may carry TRS/matrix transforms (composed down the
  hierarchy) and may reference meshes; one model may hold several meshes, several
  primitives per mesh, and one material per primitive.
* Attributes: `POSITION` (required, vec3), `TEXCOORD_0` (required, vec2),
  `COLOR_0` (optional, vec4; absent = white). Indices may be 8/16/32-bit, but every
  index must fit 16-bit addressing and the assembled mesh is capped at 65 535 vertices.
* Materials: `pbrMetallicRoughness.baseColorTexture` (optional — a material with no
  texture draws its `baseColorFactor` through the shared white sheet),
  `baseColorFactor`, `emissiveFactor`, `emissiveTexture`, and
  `KHR_materials_emissive_strength` (the only extension accepted).
* Embedded PNG images only, one decoded copy per distinct image actually used; no
  external `.bin`, no external/data-URI textures, no Draco/WebP extensions.
* UVs must be finite and inside `-0.01..=1.01` — props use **non-tiling** UVs.
* Still rejected (each with a descriptive message): skins, animations, morph targets,
  sparse accessors, non-triangle primitive modes, and any extension other than
  `KHR_materials_emissive_strength`.
* A broken model, an over-budget model, an unknown id or a missing file never fails a
  level: it draws a placeholder box instead (see [Fallback behavior](#fallback-behavior)).

Rejections are reported once per model as
`[props] {message} - using the catalogue placeholder box`; a broken model never
crashes the game, it renders a catalog-coloured box (or the neutral fallback box for
an unknown id) and the level keeps working.

### Scale, origin and orientation conventions

* **1 model unit = 1 metre.**
* The **origin is the floor-contact point, horizontally centred** under the model's
  bounding box. Base at `y = 0`.
* **+Z is the front.** Fridge doors, TV screens, vending panels and couch seats face
  `+Z` at `rotation_degrees = 0`.
* The model bounding box must match the catalog `size` within
  `max(2 cm, 6 % of the axis)`; the shipped-asset test enforces this, as it does
  base-at-origin (`|min_y| ≤ 0.012`) and centring (≤ 0.02 m).

### Budgets

| Budget | Value |
| --- | --- |
| Triangles — preferred target | 500 |
| Triangles — needs justification above | 800 (any exceedance is allowlisted explicitly in `src/props/tests.rs`) |
| Triangles — shipped art budget (tooling hard max) | **1500** (`tools/props` refuses to build above it) |
| Triangles — still loads, with an art-budget warning | above 1500, up to 6000 |
| Triangles — engine hard ceiling | 6000 (`MAX_PROP_TRIANGLES`; above it the model falls back to a box) |
| Vertices per model | 65 535 (`MAX_PROP_VERTICES`) |
| Primitives / materials / images per model | 32 / 16 / 16 |
| Prop texture | **256×256 native** (the normal shipped size; 32/64/128 legal for lighter props); engine ceiling 1024 (`MAX_PROP_TEXTURE_SIZE`), downscaled to the runtime budget at upload |
| Prop pack decoded memory | 64 MiB (`PROP_TEXTURE_PACK_BUDGET_BYTES`); the current pack is under 4 MiB |
| Materials per prop | one per primitive; a multi-material model costs one draw range per material per batch |
| Distinct models per level | 256 (fallback boxes beyond it) |
| Summed baked prop vertices per level | 1 500 000 (fallback boxes beyond it) |

The GLB tools in `tools/props/` emit and enforce the art budget; follow it. A model
above the art budget may still load if the engine ceiling allows it, but it does not
match the project's visual language and the shipped-asset tests will flag it. The
budget helpers and the preview/build commands are documented in `tools/props/README.md`.

### Fallback behavior

Unknown catalog id → placeholder box (neutral size fallback `[0.6, 0.9, 0.6]` m, but
the catalog `size` is used for the placeholder when the level does not author one).
Missing/malformed GLB, over-budget mesh, more than 256 distinct models, or exhausting
the level prop-vertex budget → placeholder box plus a one-time `[props]` warning where
a file was involved. `solid` is never affected by any of this.

### Adding a New Prop

1. Build the model with the Python toolkit (the intended route):
   add a builder to `tools/props/parts/` and register it, following the module's
   exemplar; a new module must be listed in `tools/props/parts/__init__.py`.
2. Add the catalog entry under `assets[]`:
   `id`, `display_name`, `asset_class`, optional `theme`, `asset_type: "prop"` (or
   `"entity"`), `source: "file"`, `model` relative `.glb` path, `size` `[w,h,d]`,
   `color` `#rrggbb`, `category`, `solid`.
3. Build it with the toolkit (`tools/props/README.md`).
4. Preview it and inspect the PNG.
5. Validate:
   ```sh
   python3 tools/props/build.py --check
   python3 tools/assets/validate.py
   cargo test --workspace --all-features
   ```
6. Refresh editor thumbnails if you want them (toolkit flag in `tools/props/README.md`).

A hand-authored GLB is accepted by the runtime if it satisfies the profile above, but
the default `cargo test` run enforces the origin/scale/budget conventions for every
catalogued placeable. Do not modify `spooner-man` while authoring a map; it is a
shipped entity.

---

## 17. Decals

A decal is a small decorative surface marking (a sign, a floor arrow, hazard stripes).
It is **separate geometry** laid on an existing surface, not a material edit. Its
depth handling is automatic: every decal is displaced
`DECAL_SURFACE_OFFSET_M = 2.0e-4` m (0.2 mm) along its surface normal and drawn with a
polygon offset (`glPolygonOffset(-1, -4)`, pulling it towards the camera), so it never
fights its parent surface. Do not author epsilon offsets or per-decal depth tricks.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | World centre. |
| `y` | number | no | `0.0` | Vertical centre. For `floor`/`ceiling` it is replaced by the real surface height; for walls it is the height on the wall. |
| `width` | number | **yes** | — | In-plane horizontal size, `> 0`, `≤ 10` m. |
| `height` | number | **yes** | — | In-plane vertical size, `> 0`, `≤ 10` m. |
| `rotation_degrees` | number | no | `0.0` | In-plane rotation about the surface normal. |
| `material` | string | **yes** | — | A catalog `decal` id. |
| `surface` | enum | **yes** | — | `floor`, `ceiling`, `wall_north`, `wall_south`, `wall_west`, `wall_east`. An unknown value is a JSON parse error. |

Rules:

* One flat surface per decal. A horizontal (floor/ceiling) decal that resolves to
  surfaces differing by more than 0.05 m is **rejected**
  (`Decal {i} spans a floor or ceiling height change …`). It may not straddle a
  recess edge or two rooms at different elevations.
* **No decals on gable ceilings** (`Ceiling decal {i} targets a gable ceiling …`).
* Keep decals inside the room that should light them; their corners sample that room's
  baked light. Decals are **not lightmapped**, and they have no emission term: an
  `emissive` decal sheet does not glow.
* Unknown decal ids emit nothing (no error geometry). `tools/assets/validate.py`
  reports them as level errors, so the tool is the place to catch a typo.
* Decal sheets are alpha cut-outs; background alpha 0. Artwork is visible where
  alpha ≥ 0.5 (`DECAL_ALPHA_CUTOFF`).

Use a decal when you need a **local marking on an existing surface**: signs, arrows,
hazard bands, stains that must be a specific shape. Use a material change (patch or
region) when the whole surface area changes appearance, and use geometry when the
object has depth.

### Adding a New Decal

1. Author a **POT** RGBA PNG cut-out (alpha-0 background, alpha-255 artwork). For
   project artwork, add a builder + `ART` entry in `tools/textures/decal_art.py`; for
   hand-painted art, just create the file.
2. Save it under `assets/environment/<theme>/decals/<name>_01.png` (or
   `assets/core/decals/` for shared art).
3. Add the catalog entry: `asset_type: "decal"`, `source: "file"`, `model` = the
   `.png` path.
4. Place it in the level's `decals` array with a valid `surface`.
5. Validate: `python3 tools/textures/build.py --check && python3 tools/assets/validate.py`.

---

## 18. Lighting

Places has **no dynamic lights and no realtime shadow maps**. Lighting is baked once
per level load into a per-texel lightmap atlas (with the historical baked-vertex
path as the exact fallback). Authors control it with fixture placement, fixture
type, colour and brightness — there is no level- or room-wide lighting override.
Surface response, emission, reflections and post-processing are added on top of
that bake; none of them is a light source (see section 11).

**The two halves of a glowing object are separate authored values:**

```text
visible face brightness   <- the fixture's `emission` (default = `brightness`)
visible face colour       <- the fixture's catalog sheet (texture-first; the
                             light's `color` never repaints the artwork)
illumination of the room  <- the fixture's `brightness` and `color`, only while
                             `enabled: true` (or a prop's `props[].lights`)
```

Nothing about a fixture family, a prop model or a *material* creates light. An
emissive material never casts light; a fixture face never lights a room by itself.

### The implemented lighting model

Every light is a generic engine-level source: a **shape** (point, rectangle, line), a
world position, an RGB colour, an intensity, a **range**, a **falloff** curve and an
`enabled` flag. Visible fixtures and placed props are ordinary objects that *own* zero
or more of these sources. Adding a new glowing object never means adding a new light
family.

```text
sample = room/partition-area baseline
       + visibility-tested local fixture pools
       + doorway-blend deltas
       clamped to [AMBIENT_LEVEL = 0.10, MAX_BRIGHTNESS = 1.0] per channel
```

1. **Room baseline.** Each room sums `intensity × height factor × colour` over the
   fixtures it owns, spreads it over its floor area, compresses the density and maps
   it onto `[AMBIENT_LEVEL, BASELINE_MAX = 0.60]`. The baseline is deliberately the
   *fill* level, not the highlight: it is the one term a static occluder cannot
   remove, so capping it at 0.60 leaves the visibility-tested pools (up to +0.45)
   room to read as light and shadow instead of pinning every surface at the clamp.
   A room with no fixtures sits at exactly the ambient `0.10`: unlit rooms are dark
   by design.
2. **Partitions.** If opaque internal walls split a room's footprint into
   disconnected areas, each area gets **its own baseline** from the fixtures it can
   reach. A wall that stops short of the ceiling is not a partition; a door header
   separates while a doorway keeps a bounded blend; a window does not connect
   baselines at all. The connectivity probe runs 0.15 m below the ceiling, so a wall
   must cross more than 0.75 m into the room before it is a partition candidate.
3. **Local fixture pools.** Each fixture adds a bounded local pool
   (`brightness × height factor × falloff`), evaluated over the fixture's own
   `range` (default **6 m**, clamped to `0.05`–`64` m); the summed local
   contribution is capped per channel at `0.45`. The pool is computed from the
   fixture's luminous rectangle or disc, so being near a bright fixture matters.
4. **Wall-boundary occlusion.** A pool only reaches what its fixture can see: light
   is tested against the exact wall solids. Doors, windows, passages and vents all
   transmit through exactly the hole they cut; a solid header/sill still blocks.
5. **Doorway baseline transfer.** Only `door` and `passage` openings whose bottom
   reaches the lower connected floor blend a bounded amount of the neighbour's
   baseline through the aperture (radius 6 m, strength 0.5, fading over 1 m above the
   header). Windows and vents transmit pools only; they never blend baselines.
6. **Vertical isolation.** Floors and ceilings are light boundaries: stacked rooms do
   not light each other through a slab, in brightness or colour. A raised platform or
   lowered basin inside one room volume is not a barrier, and an open side of an upper
   floor transmits normally. When rooms share a footprint, author a light `y` to pick
   the storey.
7. **No ambient control.** The 0.10 neutral floor is fixed; you cannot author sun,
   sky, a room-wide brightness or a room-wide tint. Express mood per fixture.

### Baked lightmaps

The value above is stored per *texel* of static geometry instead of per vertex: the
engine packs every floor, ceiling, wall face, reveal and recess skirt into a lightmap
atlas at level load, and the surface shader multiplies its texture by the atlas. The
lighting model, the fixtures and everything a map authors are unchanged — this is a
storage change, not an authoring one.

* Density follows the quality profile: **Full** bakes 16 texels per metre onto up to
  two 1024-texel pages, **Low** bakes 9 texels per metre onto two 512-texel pages.
  Both profiles bake the same set of surfaces; faces longer than one chart are split
  automatically (chart span cap 63.75 m). The packer is a deterministic bottom-left
  skyline, so `places_demo` fits two Full pages at 54 % data occupancy and two Low
  pages at 69 %.
* Shadow softness follows the same profile: a local pool's visibility is sampled on
  the fixture's own emitting rectangle — **Full** uses a five-tap quincunx (the
  centre plus the four quadrant corners) and **Low** the historical single centre
  tap — so a partially blocked pool fades over a penumbra instead of ending on a
  hard line where the profile pays for it. `taps_per_axis == 1` is the historical
  centre-only test, which is also what the vertex-lit fallback bakes with.
* A chart's texels span their own patch: the first and last texel sit exactly on the
  patch's geometric edges. Adjacent coplanar surfaces therefore evaluate the *same*
  world point on a shared edge, so changing an albedo material across one continuous
  floor changes the texture and leaves the baked illumination continuous. Charts stay
  separate wherever the lighting is genuinely discontinuous (a 90-degree corner, a
  wall, a different room).
* `settings.json` carries `"lightmaps": true|false` (default `true`), exposed as
  Settings → Graphics → Lightmaps and switched live (the level's lighting is
  rebuilt from the resident definition, with the player state preserved). The
  environment override `PLACES_NO_LIGHTMAPS=1` forces the historical vertex-lit
  path for a benchmark or A/B capture run.
* If a bake cannot fit the page budget, or an atlas page cannot upload, the level
  rebuilds with `LightmapMode::Off` and draws exactly the old vertex-lit colours — a
  level never renders black because of a lightmap failure. A single quad with no
  usable area (a sub-millimetre trim sliver) is *not* such a failure: it is invisible,
  so the plan leaves that quad vertex-lit and reports it in the `[lightmaps]` line
  (`left N sub-texel sliver quad(s) vertex-lit`) while the rest of the level keeps its
  atlas. A visible malformed quad (a bow-tie) still fails the build over.
* Fixtures, prop placeholder boxes, decals and the dynamic object are vertex-lit:
  their colour keeps the baked light folded in, exactly as before. Glass panes and
  stairs are lightmapped like the wall around them.
* Set `PLACES_DUMP_LIGHTMAPS=1` to write the baked atlas pages as PNGs under
  `target/agent-work/atlases/` for inspection.

#### Static props occlude the bake

A placed prop is not air: every prop's own triangles become a small set of
occlusion boxes for the bake, automatically and per distinct model. Nothing is
authored and there is no per-prop occlusion flag. The boxes are ground on a
quality-profile grid — **Full** 0.075 m, **Low** the historical 0.15 m — so Full
resolves a finer contact silhouette and the shadow a prop throws on the floor or
wall behind it, while Low keeps the cheaper derivation. The visible
consequences:

* the floor under a machine, desk or couch is darker than open floor at the same
  distance from a fixture (contact darkening);
* a large object blocks the pool behind it — a fridge or vending machine throws a
  real shadow onto the wall and floor behind it;
* furniture pushed into a corner darkens that corner, so props read as standing
  *in* the room rather than pasted onto it;
* a rotated prop shades along its rotation, not along its bounding box;
* a prop's own `lights[]` still cast normally, and are themselves blocked by the
  prop body.

Nothing about the level format changed for this, so no existing map needs an
edit. Two consequences to expect when reviewing an existing map: a fixture that
was previously lighting straight through a machine now does not, and a prop that
is *not* solid still occludes light (occlusion follows the drawn model, not the
collision box).

#### Dynamic objects (engine-created demonstration)

The engine has a separate render path for objects whose transform changes every
frame — moving components that must not be re-baked, re-batched or written into
the static lightmap. It is proven by one generated object: a `core:washer_drum`
turning in front of every placed `core:washing_machine` (the machine itself is an
ordinary static prop and participates in the bake).

* Dynamic objects are engine-created, not authored in level JSON. Placing a
  `core:washing_machine` is the only way a level influences one.
* They are lit by a single probe of the static bake at their current position
  (no shadows, no realtime lights).
* Moving one never rebuilds geometry, batches or lightmaps.

### Animated emissions

A level can make a surface's **emission** move over time: a backlit sign that
breathes, a tube on a failing ballast. It is a level-level array, keyed by
material id. This is real content from `assets/levels/places_demo.json` (the
`comment` keys shown there are ignored extras; they are omitted here):

```json
"animated_emissions": [
  { "material": "core:glass_sign_lit_01",     "effect": "pulse",   "hz": 0.09, "depth": 0.18, "phase": 0.0 },
  { "material": "core:glass_sign_flicker_01", "effect": "flicker", "hz": 7.5,  "depth": 0.6,  "phase": 0.31 }
]
```

| Field | Meaning |
| --- | --- |
| `material` | the material whose emissive term animates. **Not validated against the materials the level uses**: an id that is not in the level's material table is silently ignored at render time. `validate.py` checks only that it is a well-formed id. |
| `effect` | `pulse` (a slow sinusoid) or `flicker` (an occasional, bounded stutter). Omitted means `pulse`; an unknown name is rejected by the loader. |
| `hz` | cycles per second, must be finite, `> 0` and `≤ 24` **for both effects**. A `pulse` above 2 Hz is accepted by the loader and then clamped to 2 Hz at render time; `validate.py` rejects it up front. Omitted uses the effect default (`pulse` 0.12, `flicker` 9.5). |
| `depth` | how far the emission may fall below its authored value, must be finite, `> 0` and `≤ 0.85`. Omitted uses the effect default (`pulse` 0.22, `flicker` 0.55). |
| `phase` | a phase offset in cycles, so two signs do not breathe in lockstep; must be finite. Default `0.0`. |

Four things to know:

* **Emission only.** The animation scales the additive emissive term. The baked
  illumination is static by design, so a flickering panel keeps lighting the room
  exactly as it was baked — the fixture blinks, its pool of light does not. That
  is a deliberate, and rather liminal, property of the engine.
* **Deterministic.** Both shapes are pure functions of the level's elapsed
  seconds and start at full brightness, so the first frame is always the authored
  image and the same clock reading always produces the same picture. The clock
  advances from the simulation's delta, so a later frame number does not pin the
  phase of a running animation — only `PLACES_CAPTURE_FRAME=1` does.
* **Subtle by default.** `depth` is how *far down* the emission goes, so a pulse
  at `0.18` is a 9 % average drop and a flicker at `0.6` stutters to 40 % about a
  tenth of the time.
* **Unknown-material entries do nothing.** If the material is not resolved by the
  level, the entry costs nothing and changes nothing; check the console for
  material-resolution warnings when a sign was supposed to breathe.

### Current Light Fixture Types

Generated from `src/lighting/tuning.rs::fixture_profile` and `assets/catalog.json`.
The fixture's **face PNG is data**; its **mesh family and luminous footprint are
code**.

| Fixture ID | Mount type | Visible artwork (PNG) | Shape / footprint | Important authoring notes |
| --- | --- | --- | --- | --- |
| `core:fluorescent_panel_01` | ceiling (default) | `environment/office/textures/lights/fluorescent_panel_01.png` (1024×512) | Rectangle 1.2 × 0.6 m (half extents 0.6 × 0.3); rotation swaps axes | The default family **and the fallback for every unknown id**. The fitted sheet is the whole visible fixture (no generated bezel beside it). Hangs 0.01 m below the local ceiling; under a gable it follows the eave/ceiling above its footprint. |
| `core:pool_light_round` | ceiling | `environment/pool/textures/lights/pool_light_round_01.png` (128×128) | Disc, 0.44 m diameter (half extent 0.22); rotation-invariant | Round recessed downlight. Same ceiling-plane derivation as the panel. |
| `home:ceiling_light_round` | ceiling (default) | `environment/home/textures/lights/ceiling_light_round_01.png` (256×256) | Disc, 0.32 m diameter (half extent 0.16); rotation-invariant | Round residential flush mount: a shallow white drum with a glowing diffuser disc, hanging 0.07 m below the ceiling plane. Its pool is the disc's bounding square, as with the pool downlight. |
| `core:pool_light_wall` | **wall** — requires `"mount": "wall"` and a finite world `"y"` | `environment/pool/textures/lights/pool_light_wall_01.png` (128×64) | Rectangle 0.4 × 0.18 m (half extents 0.20 × 0.09) centred on (x, y, z) | Faces `rotation_degrees`: 0 = +Z, 90 = +X, 180 = −Z, 270 = −X. Place the point on the wall plane; the body extends ~0.11 m forward. Light is emitted from the rectangle's front. |

Ceiling fixture rotation is quantised to a 0°/90° axis swap; only wall sconces rotate
continuously. Unknown fixture ids load as the office panel with the untextured white
sheet (no error); a named-but-broken sheet logs
`[fixtures] fixture {id} sheet {path}: {error}; drawing the untextured sheet instead`.
The first light of a family decides that family's sheet for the whole level.

### The residential flush mount

`home:ceiling_light_round` is a ceiling fixture: a shallow white drum hanging
0.07 m below the ceiling plane with a glowing diffuser disc. Its visible face is
its own 256×256 sheet whose inscribed circle is the disc, so the artwork — a
soft neutral white with a moulded rim — *is* the lamp's appearance; the lit face
carries only a neutral emission strength, and the authored light `color` never
tints it. The drum wall, its bottom rim and the small centre boss behind the
diffuser's centre hole are untextured body geometry, exactly like the pool
downlight's bezel.

Its emitting footprint is the disc's bounding square (0.32 m), and like every
fixture the illumination is `brightness` (default 1.0; the Home showcase uses
0.34–0.40) while `emission` controls how bright the face reads (the showcase
authors `emission: 1.0` so the diffuser reads as a lit lamp in a softly lit
room).

A fixture family is **visible geometry plus one shape** (see
`FixtureProfile::shape` in `src/lighting/tuning.rs`): the family decides the emitting
rectangle, and everything else about the light — colour, intensity, range, falloff,
enabled state — is authored per placement. A prop needs no family at all: it owns
generic light sources directly (section 21).

### Adding a New Light Fixture Type

Fixture geometry is code. To add a family, touch each of these:

1. `src/lighting/tuning.rs` — add a `FixtureKind` variant, a stable `index()`
   (append only; it is the sheet/pipeline slot), include it in `FixtureKind::ALL`,
   add its `FixtureProfile` (`half_width`, `half_depth`, `quads`), map its logical id
   in `fixture_profile`, and append the id to `LIGHT_FIXTURE_IDS`.
2. `src/render/common/fixtures.rs` — implement the family's emitter(s): the visible face
   (the fitted PNG) into the `lit` batch, and only genuine untextured body
   geometry (a can, a housing) into `housing`. The office panel has no housing:
   its sheet is the whole fixture.
3. `src/render/common/geometry.rs` (`emit_fixtures`) — add the `match` arm that calls the new
   emitter.
4. Add the PNG under `assets/environment/<theme>/textures/lights/` (POT, opaque,
   match the face aspect).
5. Add a `light` catalog entry whose `model` is that PNG (no separate `texture`
   entry).
6. Update the tests that pin the current three families:
   `src/render/tests.rs` (sheet slots, quad counts, fitted aspect),
   `src/loader/tests.rs` (per-family sheet resolution),
   `src/assets/tests.rs` (catalog ↔ `LIGHT_FIXTURE_IDS` consistency),
   `src/lighting/tests.rs` (footprint/mount cases).
7. If the toolkit should generate the art: add a painter to
   `tools/textures/lights_art.py` and run the texture check.
8. If the editor should author/preview it: update `level-editor/js/lighting.js`,
   `model.js` and `app.js` (the editor currently mirrors only the office panel).
9. Validate: `python3 tools/textures/build.py --check`,
   `python3 tools/assets/validate.py`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`.

No catalog/renderer change is needed for a **pack** to restyle the panel family: a
`.zip` level pack may reference `"fixture": "pack:<id>"` and ship its own PNG; `pack:`
ids always resolve to the office-panel geometry.

---

## 19. Prop Placement

Exact level syntax (all fields verified against `src/level.rs::PropDef`):

```jsonc
{
  "model": "core:desk",          // REQUIRED. Logical catalog id. Unknown ids render a placeholder box.
  "x": 4.6, "y": 0.0, "z": 5.8,  // optional, default 0. y is an offset ABOVE the local walkable floor.
  "rotation_degrees": 180.0,     // optional Y rotation; model +Z faces this way at 0.
  "scale": 1.0,                  // optional, default 1.0, must be > 0. Scales model and explicit size.
  "size": [1.6, 0.75, 0.7],      // optional [w,h,d] metres for collision/placeholder.
  "solid": true,                 // optional, default false. Only this flag creates collision.
  "lights": []                   // optional 0..8 generic light sources, see section 21.
}
```

Semantics:

* `y` is **not absolute world Y**: `base_y = walkable floor at (x,z) + y`. A negative
  `y` deliberately sinks a prop into the floor and is never corrected. On a room with
  `floor_y: -1.5`, a prop at `y: 0` stands on that room's floor.
* **Collision box = level `size` (or `PROP_FALLBACK_SIZE [0.6, 0.9, 0.6]`) × `scale`.**
  The catalog `size` is never used for collision. A solid prop that should block like
  its picture must author `size`. Validation rejects non-positive/non-finite `size`
  and `scale`.
* The collision box is **axis-aligned and does not rotate**. For a 90°/270° rotated
  solid prop, author the x/z-extents swapped.
* Rotation does rotate the rendered model around Y.
* The **placeholder box** (unknown id, missing model or over-budget model) uses the
  level `size` when authored, else the catalog `size`, × `scale`.
* `props` are never tested against their render mesh for placement; intentional
  clipping and overlap are preserved.
* **Every placed prop occludes baked lighting**, automatically, from its rendered
  model: the floor under it darkens, it blocks the fixtures behind it, and it
  darkens the wall it stands against. `solid` controls collision only — a
  non-solid prop still occludes, because the occlusion comes from the drawn
  geometry. See [Static props occlude the bake](#static-props-occlude-the-bake).

Known-valid examples:

```json
{ "model": "core:desk", "x": 4.6, "y": 0.0, "z": 5.8,
  "rotation_degrees": 180.0, "size": [1.6, 0.75, 0.7], "solid": true }
```

```json
{ "model": "core:pool_guardrail_straight", "x": 14.0, "z": 9.9,
  "size": [2.0, 1.05, 0.08], "solid": true }
```

A deliberate sunken prop (from the generated prop showcase fixture; the `id` key shown
there is an ignored extra — do not copy it):

```json
{ "model": "core:crate", "x": -10.9, "z": -5.4, "rotation_degrees": 12.0,
  "y": -0.12, "solid": true }
```

`spooner-man` places through the same pipeline (it is an entity, not a prop).

---

## 20. Decal Placement

```json
{
  "x": 10.5, "y": -1.5, "z": 9.2,
  "width": 0.9, "height": 0.9,
  "rotation_degrees": 0.0,
  "material": "core:decal_no_diving_01",
  "surface": "floor"
}
```

* `x`/`z` are the decal **centre**, not a corner.
* `surface` fixes the plane and normal; `rotation_degrees` spins the artwork in that
  plane. At rotation `0` the sheet's horizontal axis runs along the surface's own
  reference direction: world **+X** for floors and ceilings, and the face's
  left-to-right direction for a viewer standing in front of a wall.
* Floor/ceiling decals snap to the real surface height under them and lift 0.2 mm;
  keep `y` consistent with the room for readability but it is replaced (a floor decal
  on a room at `floor_y: -1.5` conventionally writes `"y": -1.5`).
* Wall decals use the authored `y` as the height on the wall.

Known-valid examples:

```json
{ "x": 14.5, "y": 0.1, "z": 7.15, "width": 0.9, "height": 0.9,
  "material": "core:decal_no_diving_01", "surface": "wall_south" }
```

```json
{ "x": 24.8, "y": -1.5, "z": 10.0, "width": 0.8, "height": 1.4,
  "rotation_degrees": 90.0, "material": "core:decal_stripes_01", "surface": "floor" }
```

---

## 21. Light Placement

All fixtures — ceiling and wall — live in the level's `ceiling_lights` array. The key
name is historical; **`lights` is accepted as a serde alias** for the same array.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `fixture` | string | **yes** | — | Fixture id from the catalog/registry. Unknown ids render and bake as the office panel with the untextured sheet. |
| `x`, `z` | number | **yes** | — | World position. Must be finite. |
| `rotation_degrees` | number | no | `0.0` | Y rotation. Ceiling families quantise to a 0°/90° axis swap; wall fixtures rotate continuously. |
| `brightness` | number | no | `1.0` | Alias `intensity`. Must be finite and `≥ 0`; baking clamps to `8.0`. |
| `color` | `[r,g,b]` | no | `[1.0, 0.96, 0.88]` | Each channel `0`–`1`. Drives both the lamp face and the illumination. |
| `mount` | `"ceiling"` \| `"wall"` | no | `"ceiling"` | Closed enum. Wall fixtures require `y` or the level is rejected. |
| `y` | number | no (required for wall) | derived for ceiling | Ceiling: optional mounting world Y (also selects a storey in stacked rooms). Wall: required world Y of the fixture centre. |
| `range` | number | no | `6.0` | Distance in metres at which this fixture's pool reaches zero; must be positive and finite; clamped to `0.05`–`64`. The falloff curve is evaluated over this range, so a shorter range is also a softer pool. |
| `falloff` | `"smooth"` \| `"linear"` \| `"constant"` | no | `"smooth"` | Closed enum. Pool decay curve. `constant` holds full strength to `range` then stops (a deliberately hard pool). |
| `enabled` | boolean | no | `true` | `false` keeps the fixture's visible glow but removes **all** of its environmental illumination. |
| `emission` | number | no | the fixture's `brightness` | Independent emissive strength of the visible face, finite, `≥ 0`, clamped to `8.0`. Lets a face read brighter (or dimmer) than the light the fixture casts. |

Ceiling fixture, office default look (Places Demo, office room with red emergency
light):

```json
{ "fixture": "core:fluorescent_panel_01", "x": 21.5, "z": 2.4,
  "rotation_degrees": 0.0, "brightness": 0.34, "color": [1.0, 0.2, 0.15] }
```

Round pool downlight (Places Demo):

```json
{ "fixture": "core:pool_light_round", "x": 9.0, "z": 9.0,
  "brightness": 0.85, "color": [0.55, 0.78, 1.0] }
```

Wall fixture (must author `mount` and `y`; the point sits on the wall plane).
Places Demo:

```json
{ "fixture": "core:pool_light_wall", "x": 0.15, "z": 13.0,
  "rotation_degrees": 90.0, "brightness": 0.7, "color": [0.55, 0.78, 1.0],
  "mount": "wall", "y": 1.9 }
```

Stacked-storey selection (ceiling fixtures only):

```json
{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0, "y": 6.2 }
```

Invalid values are rejected with named messages (`Wall light {i} needs a world height
(\`y\`)…`, `Ceiling light {i} intensity cannot be negative`, `… colour channels must be
finite numbers between 0 and 1`, `Ceiling light {i} range must be a positive finite
number of metres`, `Ceiling light {i} emission must be a finite number that is not
negative`). Unknown fixture ids are not rejected; they render and bake as the office
panel with the untextured sheet.

A fixture whose face should glow without lighting the room — a sign, a screen, a
decorative tube — authors its own emissive strength separately:

```json
{ "fixture": "core:fluorescent_panel_01", "x": 47.5, "z": 13.0,
  "brightness": 0.18, "emission": 1.0 }
```

That entry is real content in `places_demo.json`: the far east corridor's last tube
reads fully bright while still casting only its dim 0.18 pool.

### Lights owned by props

Any placed prop may own generic light sources. `lights` is a level array on the prop
(0–8 entries; more is a level error). The emitter's **offset is scaled by the prop's
`scale`**, its yaw is added to the prop's rotation, and its shape is scaled with the
prop. It then casts light through exactly the same engine path as a fixture.

```json
{
  "model": "core:vending_machine", "x": 12.0, "z": 1.0, "rotation_degrees": 270.0,
  "size": [0.9, 1.9, 0.8], "solid": true,
  "lights": [
    { "shape": "rect", "half_width": 0.3, "half_depth": 0.05,
      "offset": [0.0, 1.35, 0.45], "rotation_degrees": 0.0,
      "intensity": 0.15, "range": 3.0,
      "color": [0.55, 0.78, 1.0], "falloff": "smooth", "enabled": true }
  ]
}
```

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `shape` | `"point"` \| `"rect"` \| `"line"` | no | **`"point"`** | Emitting shape. Authored extents are ignored unless `shape` says `rect` or `line`; the default does **not** infer a shape from `half_width`/`half_depth`/`length`. |
| `half_width`, `half_depth` | number | for `rect` | — | Half extents in the object's local X/Z, metres. Must be finite, `> 0`; clamped at use to 8 m. |
| `length` | number | for `line` | — | Tube length along local X, metres. Must be finite, `> 0` and `≤ 32` m. |
| `offset` | `[x, y, z]` | no | `[0, 0, 0]` | Centre of the emitter in the object's local frame; scaled with the object. A JSON array of exactly three finite numbers. |
| `rotation_degrees` | number | no | `0.0` | Yaw of the emitter relative to the object. |
| `color` | `[r, g, b]` | no | `[1.0, 0.96, 0.88]` | Each channel `0`–`1`. |
| `intensity` | number | no | `1.0` | Alias `brightness`; finite, `≥ 0`; clamped to `8.0` while baking. |
| `range` | number | no | `6.0` | Pool radius in metres, positive and finite; clamped to `0.05`–`64`. |
| `falloff` | `"smooth"` \| `"linear"` \| `"constant"` | no | `"smooth"` | Closed enum. Pool decay curve. |
| `enabled` | boolean | no | `true` | `false` casts nothing. |

At most 8 lights per prop. A light with malformed shape dimensions, a negative or
non-finite intensity, a non-positive range, a non-finite offset/rotation or an invalid
colour is a level error (the loader reports the prop and light index). The prop's own
**emission is a separate property of its GLB material** — a light authored here is the
only way a prop illuminates anything, and a glowing material does not imply one.

**Tooling note.** `tools/assets/validate.py` currently infers `rect` from authored
half extents and `line` from `length` when `shape` is omitted. The engine does not:
it treats such a light as `point` and ignores the extents. Always author `shape`
explicitly when you mean `rect` or `line`.

---

## 22. Composition and Art Direction

Places is a slow first-person exploration game of quiet, over-lit institutional
interiors that stop being finished around you. Keep this practical:

* **Coherent low-poly environments.** Geometry, props and textures share one scale and
  one deliberate vocabulary. Do not mix photorealistic texture detail with crude
  boxes — the lighting is baked, shadows are static, and reflections are limited to a
  couple of explicitly marked materials per level. The surface response (section 11) is
  detail *on* a surface: bumps, grime and brushed streaks, not sculpted geometry.
* **Textures complement geometry.** Surface art should read at a glance: tiles, carpet,
  wallpaper, concrete, panel ceilings. Detail is carried by pattern, tint and wear,
  not resolution.
* **Authored, not accidental.** Every prop, decal, stain and light placement should be
  there on purpose. Deterioration (stained wallpaper, damp carpet, damaged panels)
  should be authored deliberately with the matching materials/decals.
* **Empty space is content.** Layout, sightlines and the empty pool are the experience.
  Resist filling every room.
* **Repeated elements are acceptable** where appropriate: rows of fixtures, repeated
  desks, modular guardrails and curtains. Repetition is part of the institutional feel.
* **Mixed environment themes are allowed** when the map calls for them. Themes organize
  assets; they never restrict placement.
* **Scale may be subtly wrong on purpose.** Ceilings a little too low, corridors a
  little too long. Keep collision and traversal honest even when the proportions are
  dreamlike. (The default room height is 4.0 m; the shipped demo uses 2.7 m in the
  office, 3.0 m in the quiet corridor and 4.2 m in the stair hall and pool.)
* **Darkness is a tool.** Unlit rooms sit at the 0.10 ambient floor. Use fewer, dimmer
  or coloured fixtures rather than expecting global light.
* **One level, both quality profiles.** Full and Low run the same geometry, ids,
  materials and lights; Low lowers texture resolution, lightmap density, scene
  resolution and surface detail. Never author a second variant.

There is still **no water rendering and no refraction**. Implied water is
damp/damaged materials plus recessed geometry — optionally with a wet
`floor_patches` material (`core:pool_deck_wet_01`, a near-mirror `planar` sheen over
the dry tile) where puddled water should read. Windows may hold real glass (see
[Panes](#panes-glass-grilles-and-screens)), and a dull/dirty/clear/tinted pane is a
material choice, not a geometry one. Static probe reflections and one planar mirror
per frame exist and are used exactly where a material marks them; they are not a
general "reflections everywhere" feature.

---

## 23. Building New Assets for a Map

Decision tree:

| Need | Use | Procedure |
| --- | --- | --- |
| A wall/floor/ceiling appearance | Material + texture | Add PNG → texture entry → material entry (recipes below). |
| A glossy, metal, wet or bumpy surface | Material fields + an optional normal map | Add the albedo PNG as above, then `specular` / `shine` / `specular_color` and (optionally) a `normal_texture`. Recipe below. |
| A pane of glass, a grille or a backlit sign in an opening | A `blend`/`cutout` material + `glass` on the opening | Author the RGBA sheet, add a material with `alpha_mode`, then name it in `walls[].openings[].glass`. |
| A surface that should mirror the room | A `probe` or `planar` reflection material | Mark the material; the plane is derived from the geometry it is emitted on. Recipe below. |
| A local sign or marking | Decal | Add POT RGBA cut-out PNG → `decal` entry → place in `decals`. |
| A three-dimensional object | GLB prop | Toolkit (`tools/props/`) → build → `prop` entry → place in `props`. |
| A light source | Existing fixture, a prop-owned light, or a new fixture family | Reuse a fixture id (`ceiling_lights`) or add a family (see [Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)). A prop can own 0–8 generic lights. |
| Something mounted but not luminous | Prop | Model it as a GLB prop; there is no generic mounted-fixture system (no exit signs, fans or alarms as fixtures). |
| Something that pulses or flickers | Animated emission on a material | `animated_emissions[]`, section 18. |
| Sloped floors, stairs, half walls, columns, archways, rails, trim | The generic architectural pieces | Add `ramps`, `stairs`, `half_walls`, `columns`, `archways`, `guardrails`, `thresholds` or `baseboards`. Each takes ordinary material ids (section 10). |
| Arbitrary *other* structural geometry | **Not supported.** | Only rectangular rooms, walls, openings, patches, regions and the documented architectural pieces exist. Shape the environment from these; use props for details. |

Rules that apply to every new asset:

1. The PNG/GLB file exists in the asset tree **before** it is registered.
2. Register it in `assets/catalog.json` with a unique logical id.
3. Reference it from the level by logical id — never by path.
4. Run `python3 tools/assets/validate.py` and the matching `--check` tool.
5. Normal editable artwork is a real image file; do not generate it procedurally in
   code at runtime.

---

## 24. Common Geometry Mistakes

### Coplanar Floor Geometry

**Symptom:** the floor texture flickers between two surfaces as the camera moves.

**Cause:** two floor surfaces occupy effectively the same plane — duplicate rooms,
a patch overlapping an identical surface, or a sill top coincident with a floor.

**Authoring rule:** one surface per plane. Do not author a second floor to overlap an
existing one. Raised thresholds belong in `floor_regions`, not in a duplicated cap.
Adjacent rooms meeting at a doorway should have their floors meet at the shared
boundary; the renderer subtracts floor coverage from wall caps so the two floors
jointly cover the threshold exactly once.

### Duplicate Wall Surfaces / Adjoining Walls

**Symptom:** z-fighting along a wall shared by two rooms; visible seam or flicker.

**Cause:** a continued wall emitted a second coplanar face over the shared span, or a
wall end cap/reveal was emitted under an abutting wall.

**Authoring rule:** author each physical wall once. The renderer resolves coincident
collinear walls (same plane, thickness and overlapping length and height spans) into
one emission unit and clips hidden caps, but the correct authoring is a single wall.
Never place two walls with the same footprint.

### Wall End Caps Colliding With Perpendicular Walls

**Symptom:** an internal wall end flickers or shows a hidden face at a T-junction.

**Cause:** the end cap's plane coincides with the perpendicular wall's face.

**Authoring rule:** let walls butt cleanly: end the wall flush with the other wall's
face, not inside it, and do not add decorative end pieces that duplicate a face.
Hidden caps are clipped automatically, but avoid relying on it.

### Duplicate Doorway Floor Surfaces

**Symptom:** flicker only in a doorway, worst on raised thresholds.

**Cause:** a wall's sill/plinth top was drawn as a full-thickness cap at the walkable
floor plane while two room floors already covered that footprint.

**Authoring rule:** never create a threshold surface by hand. Floors meet at the
wall's centre plane; a raised threshold is a `floor_region`.

### Accidental Gaps / Unenclosed Rooms

**Symptom:** clear-colour void visible through a wall or corner; the player can walk
out of the world.

**Cause:** rooms do not generate walls; missing wall segments, gaps between walls
that were meant to share a corner, or walls placed by centre instead of min corner.

**Authoring rule:** shell every room. Place walls by minimum corner and overlap the
corners slightly (the demo uses 0.3 m-thick walls with ±0.15 m corner offsets) or
align them exactly at shared boundaries. `tools/assets/validate.py` warns when a wall
touches no room; the bench suite (`tools/bench/`, see its README) includes a capture
checker that counts near-black pixels, which catches holes in a level shell.

### Invalid Wall or Opening Dimensions

**Symptom:** the level is rejected on boot (`Wall {i} opening {j} …`), or an opening
silently does nothing.

**Cause:** `offset + width > wall.length()`, negative `sill`, non-positive dimensions,
or an opening whose vertical span misses the wall.

**Authoring rule:** measure the wall first: length = larger of `width`/`depth`;
openings start at the min corner. Openings that miss the wall vertically, or that are
clamped away by the local ceiling, are accepted but produce no cut — check the numbers.

### Floor Elevation Mismatch

**Symptom:** an unwalkable cliff at a doorway, a rim you did not intend, or a hole
under a wall.

**Cause:** two rooms meeting at an opening have floors differing by more than 0.4 m,
or the wall `y`/sill does not match either floor.

**Authoring rule:** decide the walking surface first; set room `floor_y`, wall `y` and
opening `sill` together. Differences of ≤ 0.4 m are walkable steps; larger differences
become solid rims with real transition faces.

### Ceiling / Gable Mismatch

**Symptom:** a wall top pokes through a sloped ceiling, or a flat cap floats above a
slope.

**Cause:** the wall authors a rigid `height` in a gable room, or its `y` is not the
room's floor level so it resolves the wrong ceiling height.

**Authoring rule:** omit `height` for walls that should follow the local ceiling.
Author `height` only when you deliberately want a rigid wall (e.g. a half-height
partition), and remember it is measured from the wall's own `y`.

### Overlapping Rooms

**Symptom:** strange ownership behavior (geometry from room A, lighting from room B),
double floors, or odd fixture selection.

**Cause:** overlapping room footprints are legal but ownership differs between
geometry (first room in order) and lighting (smallest area, with a `y` hint).

**Authoring rule:** overlap only deliberately (stacked storeys, balconies) and keep
the overlap minimal. Give lights an explicit `y` when storeys share a footprint.

### Accidental Prop Intersections

**Symptom:** props intersect walls or each other, or a "solid" prop does not block.

**Cause:** props are placed by centre; collision boxes are axis-aligned and never
corrected.

**Authoring rule:** intentional clipping is allowed and preserved — check it is
intentional. For solid props, author `size` matching the rendered footprint, swapped
for 90°/270° rotations.

### Exposed Window Interiors

**Symptom:** a window or vent looks into black void, or shows a room you did not
expect; a doorway shows a slice of the outside.

**Cause:** an opening is just a hole through a wall. What is behind it is whatever
geometry exists there: another room's interior, an unlit neighbouring volume, or the
void if the far side is outside every room.

**Authoring rule:** check both sides of every opening. A window into an enclosed
neighbouring room is glazing (author `glass`); a window on the outermost wall shows
the void and should be glazed or blanked. A door that connects rooms at different
elevations needs a floor surface under the threshold on both sides.

---

## 25. Common Lighting Mistakes

### Light Crossing an Opaque Wall

**Symptom:** a fixture appears to light the room behind a wall.

**Cause:** historically, local pools were distance-only. This is fixed: pools are
occlusion-tested against exact wall solids, and pool colour is occluded with the
brightness.

**Authoring rule:** trust the occlusion, but place fixtures inside the room they
should light. A fixture outside a room still lights the space it can see; fixtures
outside every room are defined but isolated.

### A Prop That Used To Be Lit Through Now Shadows

**Symptom:** after upgrading, a floor or wall behind a machine/cabinet is darker than
it used to be, and the object looks grounded instead of floating.

**Cause:** this is intended. Static props occlude baked light, derived from the
rendered model. A prop that stood in front of a fixture was previously lit as if it
were air.

**Authoring rule:** nothing to change — it is the desired result. If a space is now
too dark, add or brighten a fixture on the side that needs the light rather than
removing the prop. Note that a *non-solid* prop occludes too: occlusion follows the
drawn model, not the collision box.

### Lightmap Seam or Blotch

**Symptom:** a faint bright or dark line along where two walls meet, or a patch of
one surface's light bleeding into the next.

**Cause:** a lightmap chart boundary. Charts are padded and their gutters are filled
from the chart's own edge texels, and their texels span the patch edge to edge, so a
coplanar boundary — including one created only because two materials meet — is
continuous by construction. A blotchy, mottled or ring-shaped patch instead of a
line is a different failure: it means a sample's visibility answer is wrong (the
local pool was deleted for some samples and not others), not that a chart is
misplaced.

**Authoring rule:** do not try to fix it from the level — there is no chart authoring
control. Report it as an engine bug with the level id and camera position. A
vertex-lit fallback always exists (`"lightmaps": false` in `settings.json` or
`PLACES_NO_LIGHTMAPS=1`), so a map is never blocked by a bake problem.

### Fixture Assigned to the Wrong Room / Storey

**Symptom:** a room is unexpectedly dim while its neighbour is bright, or the wrong
storey lights up.

**Cause:** room ownership is by smallest containing area, and only an authored `y`
disambiguates stacked rooms.

**Authoring rule:** author `y` on ceiling fixtures whenever two rooms with different
`floor_y` share a footprint; place wall fixtures at the wall they belong to.

### Mismatched Fixture Height

**Symptom:** a wall light floats or is half-buried; a ceiling panel is at the wrong
level in a gable room.

**Cause:** wall fixtures use the authored world `y`; ceiling fixtures derive it from
the ceiling under their footprint (0.01 m below it).

**Authoring rule:** wall fixtures need `y` (validation requires it). Ceiling fixtures
need no `y` unless you are choosing a storey; under a gable they follow the ceiling
above their own footprint, so an off-centre panel can sit lower.

### Incorrect Wall-Light Orientation

**Symptom:** a sconce faces into the wall or across the room.

**Cause:** wall fixtures face `rotation_degrees` in world space and are not
auto-attached: 0 = +Z, 90 = +X, 180 = −Z, 270 = −X.

**Authoring rule:** place the origin on the wall plane, facing into the room. There
is no automatic snap.

### Too Few Fixtures / Unintended Darkness

**Symptom:** a room is nearly black except for the 0.10 ambient.

**Cause:** room baseline comes only from fixtures the room owns; there is no global
light and windows do not borrow baselines.

**Authoring rule:** give every room that should be lit at least one fixture; use more,
dimmer fixtures for even institutional light (Places Demo spaces them 2–4 m apart in
rows). If a dark room must stay dark, give it zero fixtures.

### Excessive Intensity

**Symptom:** surfaces blow out to white; colour washes out.

**Cause:** baseline + pool + doorway blend saturate at 1.0 per channel; baking clamps
intensity to 8.

**Authoring rule:** the shipped demo runs 0.18–0.85 brightness. Start there; treat
values above ~1.5 as special effects, not lighting.

### Vertical Light Leakage

**Symptom:** a lit lower storey brightens the sealed room above (or vice versa).

**Cause:** floors and ceilings are light boundaries; stacked rooms are sealed by
design.

**Authoring rule:** stack rooms freely; they are sealed. Raised platforms and lowered
basins inside one room stay connected — that is intended. If you want light to move
vertically, leave a genuine open side (an upper floor covering part of the footprint).

### Partitions

**Symptom:** a partitioned room's dark half still glows with the lit half's baseline.

**Cause:** historically one baseline per room footprint. Now baselines flood-fill
around opaque internal walls.

**Authoring rule:** an opaque internal wall that reaches the ceiling partitions the
room; a wall that stops short of the ceiling does not, because the flood fill probes
just below the ceiling. (The bake skips the flood fill entirely for rooms whose
walls only hug the boundary: a wall must cross more than 0.75 m into the room before
it is a partition candidate.) Doorways still blend a bounded amount by design.

### Doorway Transfer Assumptions

**Symptom:** light does not cross where an opening exists, or crosses a wall where a
header should block.

**Cause:** `window`/`vent` transmit pools but do not blend baselines; solid headers
block; only `door`/`passage` whose bottom reaches the lower floor blend.

**Authoring rule:** use `door`/`passage` for real room connections where you want
baseline sharing; use `window`/`vent` for apertures that should only pass local
light. Coloured light is not a global wash: it comes from specific fixtures.

### A Glowing Fixture That Lights Nothing

**Symptom:** a panel reads bright but the room stays at ambient.

**Cause:** `enabled: false` (illumination off), `brightness` near zero, the fixture
outside every room, or the fixture assigned to another storey.

**Authoring rule:** `emission` controls only how bright the face reads; `brightness`
controls the light it casts; `enabled: false` removes the cast entirely. Check all
three, plus `y` and the room it resolves to.

### An Animated Sign That Does Not Animate

**Symptom:** an `animated_emissions` entry has no visible effect.

**Cause:** the entry's `material` is not a material the level actually uses (entries
are silently ignored), the material does not emit, or the effect's `depth` is too
small to notice.

**Authoring rule:** animate a material that is on a surface in the level and has
`emissive`; start from the demo's depth values.

---

## 26. Common Asset Mistakes

### Raw File Path Instead of Logical Asset ID

**Symptom:** `[materials] …: unknown material …` and the magenta/black diagnostic
pattern; or an unknown prop rendering a placeholder box.

**Cause:** a level `material`/`model`/`fixture` contains a filename or path.

**Prevention:** levels store only catalog ids. A `.png`/`.glb` path in a level is
always wrong.

### Missing Catalog Entry

**Symptom:** broken surface, missing decal, placeholder prop, or a validation failure
from `tools/assets/validate.py`.

**Cause:** the asset file exists but is not registered (or is registered with an
unrelated id).

**Prevention:** every material/texture/decal/light/prop referenced by a level must have
an entry, and `validate.py` proves it. Run it before declaring the map finished.

### Duplicate Logical ID

**Symptom:** the catalog refuses to load
(`duplicate asset id \`{id}\` in the asset catalog`) or tooling fails.

**Prevention:** ids are globally unique across `assets` and the legacy `props` array.
Grep the catalog before adding.

### Wrong Asset Type

**Symptom:** `\`{id}\` is a \`{type}\` asset, not a surface material` (diagnostic
texture), or a fixture/prop renders a fallback.

**Prevention:** materials must be `asset_type: "material"` with a `texture`; textures
`"texture"` with a `.png` `model`; decals `"decal"`; lights `"light"`; placeables
`"prop"`/`"entity"`. `tools/assets/validate.py` errors on unknown types.

### Missing or Invalid Texture

**Symptom:** magenta/black diagnostic surface or decal, or a named console error.

**Cause:** the `texture` id dangles, the `.png` is missing, truncated, or over 1024
px; or a decal sheet is not a `.png`.

**Prevention:** run `python3 tools/textures/build.py --check` (existence, PNG
validity, 1024 hard limit) and keep the files in the repository.

### Incorrect Texture Dimensions

**Symptom:** tooling errors for >1024; warnings for >256 or non-POT fitted sheets;
stretching if a surface sheet is non-square.

**Prevention:** surfaces square, ≤1024 (256 preferred); decal sheets and fixture faces
POT; decals/fixtures are fitted, so author complete artwork with no bleed margin.

### Broken GLB

**Symptom:** `[props] prop model {path} is invalid: {reason}` and a placeholder box.

**Cause:** extensions other than `KHR_materials_emissive_strength`, skins, animations,
morph targets, sparse accessors, external textures, non-triangle primitives,
>65 535 vertices, >6000 triangles, >32 primitives, >16 materials/images, or a
texture edge >1024.

**Prevention:** build props with `tools/props/`, preview them, and run
`cargo test --workspace --all-features` so the shipped-asset checks enforce the
conventions. (Multiple meshes, multiple primitives and multiple materials are
supported — the old single-mesh restriction is gone.)

### Excessive Model Budget

**Symptom:** art-budget warnings; a model that does not match the low-poly visual
language; possible engine rejection.

**Prevention:** 500 triangles preferred, 800 justified, 1500 shipped art budget;
256×256 is the native prop texture size (32/64/128 legal for lighter props), and
the whole pack must decode to at most 64 MiB. Exceedances are allowlisted
explicitly in `src/props/tests.rs`.

### Texture Seams / Incorrect Tiling

**Symptom:** a visible grid every `tile_metres`; content visibly repeats at the wrong
scale.

**Cause:** the PNG edges do not wrap (seam), or `tile_metres` does not match the
intended real-world size.

**Prevention:** author tileable art; run
`python3 tools/textures/seam_repair.py --check <png>`; set `tile_metres` to the real
period (pool deck 1.5, basin/wall 1.0, office surfaces 2.0).

### Incorrect Alpha

**Symptom:** a decal shows its background plate (alpha not cut out); a surface
renders unexpectedly transparent (an `opaque` material ignores the alpha channel by
design).

**Prevention:** decal sheets are RGBA cut-outs with background alpha 0 and artwork
alpha 255; surface and fixture images are opaque unless their material authors
`alpha_mode`.

### Generated Artwork Instead of a File

**Symptom:** a texture that only exists in code; a review rejection; a shipped asset
check that cannot find the PNG.

**Cause:** the texture was drawn procedurally in Rust/Python at runtime instead of
being stored as a PNG.

**Prevention:** every permanent texture is a real `.png` in `assets/` and is named by
a catalog entry. The only generated images are the diagnostics listed in
[Textures](#12-textures).

---

## 27. Validation Workflow

Run from the repository root. The commands below are the current required checks;
none of them is optional for a change that ships content.

| Command | What it validates | Required for map authoring? |
| --- | --- | --- |
| `python3 tools/assets/validate.py` | Catalog parse; classes/types/sources; unique ids; every file-backed resource exists exactly once; material texture/mask/normal references; shipped/drop-in/fixture levels reference declared ids; prop-light and fixture-pool shapes/fields; animated-emission schema; warns when a wall touches no room | **Yes** |
| `cargo test --workspace --all-features` | The whole Rust suite: level/loader/render/material/collision/lighting tests plus the audits (surface, lighting, isolation, parity, partition, vertical, leak) | **Yes** |
| `cargo fmt --all --check` | Rust formatting | **Yes** when code changed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Strict lints (`AGENTS.md` policy) | **Yes** when code changed |
| `python3 tools/textures/build.py --check` | Texture/decal/fixture PNGs exist, parse, ≤1024; warns >256 / non-POT | Yes when art changed |
| `python3 tools/props/build.py --check` | Every catalogued prop GLB exists and parses, and its decoded texture memory fits the per-texture and 64 MiB pack budgets; prints bounds/budget flags | Yes when props changed |
| `PLACES_LEVEL=<id> cargo run` | Boots straight into the level and prints validation errors verbatim | **Yes, once per map** |
| `PLACES_CAPTURE=frame.png PLACES_LEVEL=<id> cargo run` | One-frame PNG capture for visual inspection (`PLACES_CAPTURE_FRAME=n` waits for frame n first) | Useful |
| `python3 tests/test_package.py` | Repository/package gate: shipped-level checks, texture policy, catalog validation, README hygiene | Recommended before shipping a map into `assets/levels/` |
| `PLACES_DUMP_LIGHTMAPS=1 PLACES_LEVEL=<id> cargo run` | Writes the baked atlas pages as PNGs under `target/agent-work/atlases/` | Useful |
| `python3 tools/textures/seam_repair.py --check <png>` | Tiling seam metric per texture | Yes for new surface art |
| `tools/bench/README.md` | Index of the current benchmark and capture tools — it is the authoritative, current list | Useful |

Do not treat a clean `--check` as style approval: `tools/props/build.py --check`
enforces texture memory and container validity, while the triangle/scale/origin
art budgets live in `cargo test`. Conversely, `validate.py` is stricter than the
runtime about catalog classes/types/sources and is the tool that catches a
dangling level reference.

### What `tools/assets/validate.py` checks (and what only Rust checks)

`validate.py` is not a headless replacement for the Rust loader. It checks:

* the catalog: themes, unique ids, known classes/types, `source`, relative model
  paths that exist, `.png` models for textures/decals/fixture faces, materials
  resolving to file-backed textures, `emissive_mask`/`normal_texture` references,
  the canonical `spooner-man`, and the numeric ranges listed in
  [Asset Catalog](#14-asset-catalog);
* levels: every referenced asset id is declared (defaults, rooms, regions, walls,
  faces, opening `glass`, patches, decals, fixtures, props, animated-emission
  materials), prop lights and fixture pool/emission fields, animated-emission schema,
  and a warning when a wall touches no room.

Only the Rust loader enforces: format version, identity, spawn finiteness, room/
wall/region/opening dimensions and bounds, floor-region containment and eave rule,
prop/light/decal schemas, the geometry budgets, limits and caps, gable rules, decal
surface rules, and the walkable/collision behaviour. Boot the level to prove those.

### Environment switches

These are session switches, not authoring fields. They exist for benchmarking,
capture and diagnosis.

| Switch | Effect |
| --- | --- |
| `PLACES_LEVEL=<id>` | Boot straight into a level and print its validation errors. |
| `PLACES_QUALITY=full\|low` | Draw this run at the named quality profile without editing `settings.json`, so both profiles of the same level can be captured back to back. The Settings screen shows the overridden profile (marked `*`) and changing it there clears the override. |
| `PLACES_CAPTURE=<file.png>` | Write one frame as a PNG and exit. |
| `PLACES_CAPTURE_FRAME=<n>` | Capture frame n (1-based) instead of the first; also pins animation phase. |
| `PLACES_NO_LIGHTMAPS=1` | Force the historical vertex-lit path for this run (overrides the Lightmaps setting). |
| `PLACES_DUMP_LIGHTMAPS=1` | Write the baked atlas pages to `target/agent-work/atlases/`. |
| `PLACES_NO_OFFSCREEN=1` | Draw the 3D scene straight into the window instead of through the offscreen target. **Not pixel-identical any more:** this path skips the planar reflection pass, the reflection-probe binds, bloom and the resolve, and it ignores the Low profile's reduced scene resolution. It exists for benchmark A/B runs and driver bring-up. |
| `PLACES_NO_BLOOM=1` | Keep the resolve pass but drop the emissive bloom pass and blur for this run (overrides the Bloom setting). |
| `PLACES_NO_REFLECTIONS=1` | Report every material as reflection-free: no planar pass, no probe bake, no reflection binds, for this run (overrides the Reflections setting). |
| `PLACES_ASSET_ROOT=<dir>` | Override the directory that contains `assets/`. |
| `PLACES_STATE_ROOT=<dir>` | Override the directory that owns `settings.json`, drop-in `levels/` and `import/`. |
| `PLACES_VERBOSE=1` | Print the developer telemetry (package, asset, level-build, lighting, lightmap and framing lines). Unset, a normal run is silent; problems are still reported once each. |

A level that fails validation is reported at discovery
(`[levels] skipping {path}: {reason}`). Always boot with `PLACES_LEVEL=<id>` to read
the error in full.

---

## 28. Final Map QA Checklist

### Structure

- [ ] Spawn is inside a real room, at a sensible position, facing the intended direction.
- [ ] Every space the player can enter is fully shelled by walls (no void gaps).
- [ ] Rooms connect intentionally; every connection has an opening in the correct wall.
- [ ] Room elevations match the intended route; no unwalkable surprise cliffs (or the
      cliffs are intended and have real rims).
- [ ] Openings fit their walls: `0 ≤ offset`, `offset + width ≤ length`, `sill ≥ 0`,
      positive dimensions, ≤ 64 per wall.
- [ ] No accidental gaps at wall corners or wall ends.

### Geometry

- [ ] No unintended coplanar surfaces (duplicate walls, duplicate floors, duplicated
      thresholds).
- [ ] No Z-fighting observed while walking the route and at doorways specifically.
- [ ] Doorway thresholds are owned once by the two room floors; raised thresholds are
      floor regions.
- [ ] Walls do not duplicate each other; wall ends butt cleanly.
- [ ] Floor regions overlap a room and sit below its eave.
- [ ] No floor/elevation faces poke through walls or ceilings.
- [ ] Windows/vents look into geometry, not the void; glazing is authored where the
      opening should read as glazed.

### Materials

- [ ] Every material id exists in the catalog and resolves.
- [ ] The intended floor/wall/ceiling material is on the intended surface (check room
      overrides, wall `faces`, patches, regions).
- [ ] Recesses author an `edge_material`.
- [ ] Textures tile correctly at the intended real-world scale (`tile_metres`).
- [ ] No visible unintended seams; new surface art passed the seam check.
- [ ] `alpha_mode` choices are intentional; `blend` surfaces never hide a room behind
      them in the depth buffer.
- [ ] Reflection markings are deliberate: at most a couple of probe materials and at
      most one or two planar surfaces that are flat and axis-aligned.

### Props

- [ ] Each prop is at the intended position and `y` (props stand on the local floor;
      sinking must be deliberate).
- [ ] Each prop faces the intended direction (`+Z` front at 0°).
- [ ] No accidental floating or sinking.
- [ ] `solid` matches intent; every solid prop authors a `size` that matches its
      rendered footprint (x/z swapped for 90°/270° rotations).
- [ ] No accidental prop-in-prop or prop-in-wall intersections.
- [ ] Props that should illuminate author `props[].lights` (≤ 8) and their `shape` is
      explicit.

### Lighting

- [ ] Intended areas are illuminated; dark areas remain appropriately dark.
- [ ] Light does not pass through opaque walls (check both sides of shared walls).
- [ ] Coloured light stays in its own room.
- [ ] Stacked/overlapping spaces do not leak light vertically; storey fixtures author `y`.
- [ ] Wall fixtures have `mount` + `y` and face into the room.
- [ ] Fixture artwork and orientation look correct (panel faces down, sconce faces out).
- [ ] No room relies on ambient light beyond the intended 0.10 floor.
- [ ] Doorways blend as intended; windows/vents pass only local light.
- [ ] Props that should light their surroundings author `props[].lights`; a glowing
      material alone does not illuminate anything.
- [ ] A fixture meant to glow without lighting the room authors `emission` (and, if
      it must cast nothing at all, `enabled: false`).
- [ ] Emissive surfaces read bright in dark areas without brightening their neighbours.
- [ ] Animated emissions name materials the level actually uses.
- [ ] New props that shadow a previously lit area are intentional; the space still
      reads with the contact darkening.

### Assets

- [ ] Every referenced file exists.
- [ ] `assets/catalog.json` is valid and unique.
- [ ] Textures are valid PNGs within limits (surfaces square; decals/fixtures POT).
- [ ] GLBs are valid and within budgets.
- [ ] No level contains a raw file path where a logical id is required.
- [ ] New artwork exists as real files in the asset tree (not generated in code).

### Validation

- [ ] `python3 tools/assets/validate.py` exits 0.
- [ ] `python3 tools/textures/build.py --check` exits 0 (new art in particular).
- [ ] `python3 tools/props/build.py --check` exits 0 (new props in particular).
- [ ] `cargo test --workspace --all-features` passes.
- [ ] The level boots with `PLACES_LEVEL=<id>` with no validation error.
- [ ] A capture (`PLACES_CAPTURE`) has been inspected if practical, at both Full and
      `PLACES_QUALITY=low` if reflections or material response matter.

---

## Known Implementation Caveats

These are current, documented limitations or in-flight conditions that affect map
authoring. They are not invitations to change the engine as part of an authoring task.

1. **Quality profiles apply at level load.** `settings.json`'s `quality` value is read
   when the renderer is created; changing it mid-session does not re-scale textures
   already on the GPU. Set it, then load the level — or use `PLACES_QUALITY` for one
   run.
2. **Emission reaches surfaces and fixture faces, not decals.** A decal is drawn by
   its own pass, which has no emission term; an `emissive` material used as a
   *decal sheet* will not glow. Emission on wall/floor/ceiling materials and on GLB
   prop materials works.
3. **A light's range normalises its falloff.** `range` is the distance at which the
   pool reaches zero *and* the span the curve is evaluated over, so halving a range
   makes the pool both tighter and dimmer near the source. There is no separate
   "cutoff only" mode.
4. **Cone/spot lights are not implemented.** The generic model has point, rectangle
   and line shapes; a directional light needs a response model that does not exist.
5. **A GLB may embed larger prop textures than the shipped native size.** The
   engine accepts up to 1024 px per edge but Full uploads a prop sheet at 256
   and Low at 128, so the shipped toolkit stays at the 256 native size;
   authoring bigger embedded art gains nothing under Full unless the engine's
   prop budget is raised first.
6. **`emission` on a fixture is emission only.** It never changes illumination; if a
   glowing face should also light the room, that is `brightness`/`enabled`.
7. **No `deny_unknown_fields`.** Misspelled or unsupported level keys are silently
   ignored: `"rotation": 90` does nothing, `"brightnesss": 0.5` does nothing. Diff
   against the schema skeleton and the field tables.
8. **Invalid levels are reported, then skipped.** Discovery logs
   `[levels] skipping {path}: {reason}`; the level is absent from the menu. Boot with
   `PLACES_LEVEL=<id>` to reproduce.
9. **No duplicate-level-id detection.** Two files may both declare `"id": "my_level"`;
   both appear, and `PLACES_LEVEL` picks the first in the deterministic menu order
   (name, then id).
10. **Spawn outside every room is accepted** and falls back to floor `0.0`. Check it.
11. **`floor_patches` are dimension-unvalidated.** They are capped at 2000 entries,
    but a malformed patch is skipped at build time rather than rejected. Keep them
    well-formed and inside a room.
12. **A pack cannot shadow a catalog material id**, and a pack material that ships its
    own PNG currently drops `reflection_mode`/`reflection_strength` (rule a
    reflective pack surface to reuse a catalog texture). See
    [Level packs: `materials.json`](#level-packs-materialsjson).
13. **A prop light's `shape` does not infer.** Omitting `shape` makes the light a
    point and ignores `half_width`/`half_depth`/`length`. `tools/assets/validate.py`
    currently infers a shape from those fields, so a level can pass the tool and
    still bake as a point; author `shape` explicitly.
14. **Dynamic objects are not authorable.** The only dynamic object is the engine's
    demonstration drum spawned by placing `core:washing_machine`; a level cannot place
    or drive one.
15. **Documentation drift in shipped docs** (recorded here so agents trust the code):
    the prop exceedance allowlist lives in `src/props/tests.rs`, not
    `src/props.rs`;
    `src/level.rs`'s `y` comment says a ceiling fixture's `y` is ignored, but the bake
    honours it for storey selection; `src/materials/reflection.rs`'s module comment
    shows a nested `"reflection": {"mode": …}` catalog form while the catalog uses the
    flat `reflection_mode`/`reflection_strength` fields.
16. **Legacy level editor is stale for vertical and architectural keys.**
    `level-editor/js/` does not model `floor_y`, `floor_regions`, `ceiling` profiles,
    fixture `mount`/`y`, or any of `ramps`, `stairs`, `half_walls`, `columns`,
    `archways`, `guardrails`, `thresholds` and `baseboards`, and it defaults a missing
    room `height` to 3.5 (the engine default is 4.0). Prefer editing JSON directly for
    those features.
17. **Tooling vs runtime strictness.** The Rust runtime is permissive (unknown
    class/type, missing `model`, invalid `size`, unknown fixture/prop ids degrade);
    `tools/assets/validate.py` is strict and fails. Pass the tool, not the runtime
    fallback.
18. **Generated fixture quirk:** `tests/fixtures/levels/prop_showcase.json` (generated)
    carries an `id` key on props that the level schema ignores. Do not copy it.
19. **`props/build.py --check` enforces container validity and decoded texture
    memory but not the triangle/scale/origin art budgets**; those live in
    `cargo test`. Do not treat a clean `--check` as complete budget approval.
20. **No water or dynamic lighting.** "Flooded" and "mood lighting" must be expressed
    with existing materials, geometry and per-fixture colour/brightness.
21. **Reflections are per-material and limited.** One planar plane per frame, at most
    two probes per level, probes are static (no realtime update), and a planar material
    reused on non-planar geometry is skipped with a warning.

---

# Authoring Recipes

Compact, verified procedures. Every JSON snippet uses only implemented fields.
Replace ids/paths with your own logical ids; never introduce a raw path.
New-asset recipes use illustrative ids (`hotel:…`, `pool:tile_blue_01`,
`pool:decal_slip_01`) that do not exist until you create them; every example that
references shipped content uses a real catalog id.

## Add a room

1. Append to `rooms` with min-corner `x`/`z`, `width`, `depth`.
2. Set `height` (default 4.0 if omitted) and `floor_y` if the room is elevated.
3. Optionally set `material` and `ceiling_material`.
4. Remember: the room has no walls yet — add them separately.

```json
{ "x": 10.0, "z": 0.0, "width": 6.0, "depth": 5.0,
  "height": 3.0, "floor_y": -0.9,
  "material": "core:carpet_beige_01",
  "ceiling_material": "core:ceiling_panel_01" }
```

## Add a gable ceiling

```json
{ "x": 30.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 3.0,
  "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 1.6 },
  "material": "core:carpet_beige_01",
  "ceiling_material": "core:ceiling_stained_01" }
```

No decals on this ceiling; walls may omit `height` to follow the slope.

## Add a wall

1. Place by **minimum corner** `x`/`z`.
2. `width`/`depth`: the larger is the length axis.
3. `y` is the wall's absolute base; `height` omitted = follows the local ceiling.
4. Add `material` / `faces` only when overriding the level default.

```json
{ "x": 10.0, "z": -0.15, "width": 6.3, "depth": 0.3, "y": -0.9, "height": 3.0 }
```

## Add a doorway

1. Choose the wall and the offset from its min corner along +X or +Z.
2. `width`/`height` are the cut; `sill: 0.0` means walk-through.
3. Check `offset + width ≤ wall.length()`.

```json
{ "kind": "door", "offset": 1.2, "width": 1.2, "height": 2.1, "sill": 0.0 }
```

## Add a window

Same as a doorway with `kind: "window"` and a `sill` above the floor. Collision
follows the geometry, so a raised window blocks.

```json
{ "kind": "window", "offset": 1.65, "width": 2.2, "height": 1.3, "sill": 1.7 }
```

## Glaze a window

1. Name a material with `alpha_mode: "blend"` (clear, dirty, tinted or emissive);
   see [Panes](#panes-glass-grilles-and-screens) and the shipped
   `core:glass_window_*` materials.
2. Add `glass` to the opening. The pane fills the aperture at the wall's centre
   plane and samples the wall's baked light.

```json
{ "kind": "window", "offset": 1.65, "width": 3.2, "height": 1.3, "sill": 1.7,
  "glass": "core:glass_window_dirty_01" }
```

For a grille or screen instead of glass, use a `cutout` material
(`core:grille_vent_01` on a `vent` opening is the shipped example).

## Give a surface a sheen (plastic, metal, glossy tile, wet floor)

1. Start from an ordinary material. Add `specular` (how much light the surface
   can catch) and `shine` (how glossy it is); add `specular_color` only when the
   sheen should be tinted (metal). Keep ordinary floors and walls near
   `shine: 0.0`.
2. Optionally name a `normal_texture` for surface detail.
3. Do not author a light for this: the sheen is lit by whatever the bake already
   delivers to that surface.

```json
{ "id": "hotel:floor_polished_01", "asset_class": "environment",
  "asset_type": "material", "source": "definition", "surface": "floor",
  "texture": "hotel:tex_floor_polished_01", "tile_metres": 2.0,
  "specular": 0.4, "shine": 0.5 }
```

```json
{ "id": "hotel:metal_panel_01", "asset_class": "environment",
  "asset_type": "material", "source": "definition", "surface": "wall",
  "texture": "hotel:tex_metal_panel_01", "tile_metres": 2.0,
  "tint": [0.86, 0.87, 0.88],
  "specular": 0.55, "specular_color": [0.9, 0.93, 1.0], "shine": 0.3,
  "normal_texture": "hotel:tex_normal_brushed_01", "normal_strength": 0.45 }
```

To vary one surface's glossiness without a new material, put a `shine` override
on the room/wall/patch/region that names it (see
[Material references and shine overrides](#11-materials)):

```json
{ "x": 6.0, "z": 4.0, "width": 3.0, "depth": 2.0, "material": "hotel:floor_polished_01",
  "shine": 0.05 }
```

## Make a reflective surface (probe or planar)

1. Start from a sheen material: the reflection rides on `specular` and `shine`,
   and a material with `specular: 0` never reflects.
2. Add `reflection_mode`; `probe` for a curved/unknown view (static cubemap), `planar`
   for a genuinely flat, axis-aligned mirror.
3. Add `reflection_strength` (default 0.45) only when the default reads wrong.
4. Mark sparingly: only one planar plane is drawn per frame (extra planes take
   turns), and `Low` drops planar reflections entirely.

```json
{ "id": "hotel:lobby_marble_01", "asset_class": "environment",
  "asset_type": "material", "source": "definition", "surface": "floor",
  "texture": "hotel:tex_marble_01", "tile_metres": 2.0,
  "specular": 0.55, "shine": 0.4,
  "reflection_mode": "probe", "reflection_strength": 0.35 }
```

```json
{ "id": "hotel:pool_deck_wet_01", "asset_class": "environment",
  "asset_type": "material", "source": "definition", "surface": "floor",
  "texture": "hotel:tex_deck_tile_01", "tile_metres": 1.5,
  "specular": 0.55, "shine": 0.72,
  "reflection_mode": "planar", "reflection_strength": 0.3 }
```

A true mirror is a planar reflective material with a high shine and an opaque,
flat surface (a pane in an opening, or a wall slab): `shine` alone never samples
the room.

## Make a surface translucent or a cut-out

1. Author (or reuse) an RGBA sheet: the alpha channel is the coverage.
2. Set `alpha_mode`. `blend` for glass or a lit sign, `cutout` for a grid or a
   perforated panel; add `opacity` / `alpha_cutoff` only when the defaults are
   wrong.
3. Nothing else: the renderer puts the material in the right pass. A translucent
   emissive surface is just `emissive` plus `alpha_mode: "blend"`.

```json
{ "id": "hotel:glass_sign_lit_01", "asset_class": "environment",
  "asset_type": "material", "source": "definition", "surface": "wall",
  "texture": "hotel:tex_glass_tinted_01", "tile_metres": 1.0,
  "alpha_mode": "blend", "opacity": 0.9,
  "specular": 0.35, "shine": 0.6,
  "emissive": [0.86, 0.93, 1.0], "emissive_intensity": 1.35 }
```

## Change one wall face's material

1. Identify the wall axis: X-axis wall → `north`/`south`; Z-axis wall → `west`/`east`.
2. Add `faces` (wins over `material` and the default).

```json
{ "x": 18.85, "z": 0.15, "width": 0.3, "depth": 6.7, "y": -1.5, "height": 4.2,
  "material": "core:pool_tile_wall_01",
  "faces": { "west": "core:wallpaper_yellow_01" } }
```

## Lower a room / floor

Whole room: set `floor_y`. Part of a room: add a floor region with a negative
`offset_y`. Keep intentional walkable transitions at ≤ 0.4 m per step.

```json
{ "x": 0.0, "z": 7.0, "width": 26.0, "depth": 12.0, "height": 4.2,
  "floor_y": -1.5, "material": "core:pool_tile_deck_01",
  "ceiling_material": "core:pool_ceiling_01" }
```

```json
{ "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0, "offset_y": -1.5,
  "material": "core:pool_tile_basin_01", "edge_material": "core:pool_tile_wall_01" }
```

## Place a prop

1. Use a catalog `prop`/`entity` id.
2. `y` is an offset above the local walkable floor; negative sinks.
3. Author `size` for a solid prop; swap x/z for 90°/270° rotations.

```json
{ "model": "core:desk", "x": 4.6, "y": 0.0, "z": 5.8,
  "rotation_degrees": 180.0, "size": [1.6, 0.75, 0.7], "solid": true }
```

A prop that owns its own light (a machine with a lit panel or a subtle glow):

```json
{ "model": "core:vending_machine", "x": 12.0, "z": 1.0, "rotation_degrees": 270.0,
  "size": [0.9, 1.9, 0.8], "solid": true,
  "lights": [
    { "shape": "rect", "half_width": 0.3, "half_depth": 0.05,
      "offset": [0.0, 1.35, 0.45], "intensity": 0.15, "range": 3.0,
      "color": [0.55, 0.78, 1.0], "falloff": "smooth", "enabled": true }
  ] }
```

## Place a decal

1. Choose a catalog `decal` id.
2. `x`/`z` are the centre; `surface` fixes the plane.
3. One flat surface; not across height changes; not on gables.

```json
{ "x": 10.5, "y": -1.5, "z": 9.2, "width": 0.9, "height": 0.9,
  "material": "core:decal_no_diving_01", "surface": "floor" }
```

## Place a ceiling light

1. Use a fixture id; omit `mount` for ceiling fixtures.
2. `brightness` defaults to 1.0; `color` defaults to warm white.
3. Add `y` only to select a storey in stacked rooms.
4. Optional: `range`/`falloff` shape the pool, `enabled: false` keeps the glow but
   removes the light, and `emission` sets the face brightness independently.

```json
{ "fixture": "core:fluorescent_panel_01", "x": 21.5, "z": 2.4,
  "brightness": 0.34, "color": [1.0, 0.2, 0.15] }
```

A tube that reads bright but casts its dim pool (the demo's far corridor panel):

```json
{ "fixture": "core:fluorescent_panel_01", "x": 47.5, "z": 13.0,
  "brightness": 0.18, "emission": 1.0 }
```

## Place a wall light

1. Use a wall fixture id.
2. `mount` must be `"wall"` and `y` must be a finite world height.
3. Place the point on the wall plane; face it into the room.

```json
{ "fixture": "core:pool_light_wall", "x": 0.15, "z": 13.0,
  "rotation_degrees": 90.0, "brightness": 0.7, "color": [0.55, 0.78, 1.0],
  "mount": "wall", "y": 1.9 }
```

## Animate an emission

1. The material must be used by the level and must have `emissive`.
2. Add an entry to `animated_emissions`: `effect` is `pulse` or `flicker`; keep
   `hz` ≤ 2 for a pulse; `depth` ≤ 0.85.
3. Give a second sign a different `phase`.

```json
"animated_emissions": [
  { "material": "core:glass_sign_lit_01", "effect": "pulse",
    "hz": 0.09, "depth": 0.18, "phase": 0.0 }
]
```

## Package a level as a `.zip` pack

1. Put `level.json` at the pack root (the file name is required).
2. Optionally add `materials.json` and PNGs under `textures/`.
3. Declare pack materials with `pack:` ids inside `materials.json`, then reference
   those ids from `level.json`.
4. Keep the pack within the limits: ≤ 500 entries, ≤ 10 MB per entry, ≤ 50 MB total
   uncompressed. Safe extensions: `.exe`, `.sh`, `.bat`, `.so`, `.dylib`, `.dll`,
   `.bin`, `.wasm` are skipped.
5. Drop the `.zip` into `levels/` (or use the import flow) and boot it with
   `PLACES_LEVEL=<id>`.

```json
{
  "materials": {
    "pack:lobby_wall": {
      "texture": "textures/wall_lobby.png",
      "tile_metres": 2.0,
      "tint": [1.0, 0.98, 0.94]
    },
    "pack:sign_face": {
      "texture": "textures/sign_face.png",
      "tile_metres": 1.0,
      "emissive": [1.0, 0.9, 0.6],
      "emissive_intensity": 1.5,
      "alpha_mode": "blend",
      "opacity": 0.9
    }
  }
}
```

## Add a new texture

1. Create the PNG: square, opaque, tileable, ≤1024 (256 preferred), 8-bit.
2. Save under `assets/environment/<theme>/textures/{walls,floors,ceilings}/<name>_01.png`.
3. Add a `texture` catalog entry with `model` = the path relative to `assets/`.
4. Validate: `python3 tools/textures/build.py --check` and
   `python3 tools/assets/validate.py`.

```json
{ "id": "hotel:tex_wallpaper_green_01", "display_name": "Hotel Green Wallpaper Texture",
  "asset_class": "environment", "theme": "hotel", "asset_type": "texture",
  "source": "file", "surface": "wall",
  "model": "environment/hotel/textures/walls/wallpaper_green_01.png" }
```

## Add a new material

1. Ensure the texture entry exists (above).
2. Add a `material` entry naming `texture`, with optional `tile_metres` and `tint`.
3. Reference it from a level (`defaults`, room, wall/`faces`, patch, region or
   opening `glass`).
4. Optional: author `emissive`/`emissive_intensity` (and `emissive_mask`) to make the
   surface glow. Emission is not a light: add a fixture or a `props[].lights` entry if
   the surface should illuminate the room.

```json
{ "id": "hotel:wallpaper_green_01", "display_name": "Hotel Green Wallpaper",
  "asset_class": "environment", "theme": "hotel", "asset_type": "material",
  "source": "definition", "surface": "wall",
  "texture": "hotel:tex_wallpaper_green_01", "tile_metres": 2.0,
  "tint": [1.0, 1.0, 1.0] }
```

A lit sign face:

```json
{ "id": "hotel:sign_exit_01", "display_name": "Exit Sign Face",
  "asset_class": "environment", "theme": "hotel", "asset_type": "material",
  "source": "definition", "surface": "wall",
  "texture": "hotel:tex_sign_exit_01", "tile_metres": 1.0,
  "emissive": [0.35, 1.0, 0.45], "emissive_intensity": 2.0 }
```

## Add a new prop

1. Add a builder + registry entry in `tools/props/parts/<module>.py` (see the module
   exemplar and `tools/props/README.md`).
2. Add the catalog `prop` entry (`model` `.glb`, `size`, `color`, `category`,
   `solid`).
3. Build it with the toolkit.
4. Preview it and inspect the PNG.
5. Validate: `python3 tools/props/build.py --check`,
   `python3 tools/assets/validate.py`, `cargo test --workspace --all-features`.
6. Place it by logical id from a level. Do not modify `spooner-man`.

```json
{ "id": "hotel:luggage_cart", "display_name": "Luggage Cart",
  "asset_class": "environment", "theme": "hotel", "asset_type": "prop",
  "source": "file", "model": "environment/hotel/props/models/luggage_cart.glb",
  "size": [1.2, 1.1, 0.6], "color": "#6b5a44", "category": "Furniture",
  "solid": true }
```

## Add a new decal

1. Create a POT RGBA cut-out PNG (background alpha 0, artwork alpha 255).
2. Save under `assets/environment/<theme>/decals/<name>_01.png`.
3. Add a `decal` catalog entry with `model` = the `.png`.
4. Place it in a level's `decals` array with `surface`.
5. Validate with the texture and catalog checks.

```json
{ "id": "hotel:decal_evacuation_01", "display_name": "Evacuation Route Sign",
  "asset_class": "environment", "theme": "hotel", "asset_type": "decal",
  "source": "file", "model": "environment/hotel/decals/evacuation_route_01.png" }
```

## Add a ramp or a staircase

1. Choose the footprint and the run axis: the longer of `width`/`depth` is the
   run, exactly like a wall.
2. A ramp takes `offset_y` (at the minimum-corner end) and a signed `rise`; a
   staircase takes `offset_y` (at the foot) and a positive `rise` with `steps`.
3. Make the far end meet a floor region edge-to-edge at the same height (a ramp
   may not overlap one).
4. Name a `material` for the top and, for a ramp, an `edge_material` for the
   sides; stairs take tread, riser and side materials.

```json
{ "x": 4.0, "z": 0.4, "width": 1.0, "depth": 1.6, "offset_y": 0.0, "rise": 0.75,
  "material": "home:hardwood_oak_01",
  "edge_material": "home:wall_paint_offwhite_01" }
```

```json
{ "x": 2.2, "z": 2.6, "width": 1.4, "depth": 1.2, "offset_y": 0.0, "rise": 0.75,
  "steps": 5, "material": "home:hardwood_oak_01",
  "riser_material": "home:wall_paint_offwhite_01",
  "side_material": "home:baseboard_white_01" }
```

The platform it lands on is an ordinary raised floor region:

```json
{ "x": 3.6, "z": 2.0, "width": 1.8, "depth": 2.4, "offset_y": 0.75,
  "material": "home:hardwood_oak_01", "edge_material": "home:wall_paint_offwhite_01" }
```

## Add a half wall, a column or an archway

1. Place by minimum corner like a wall; the longer of `width`/`depth` is its
   length axis.
2. Give a half wall its `height` (required) and, optionally, an `end_material`
   and a `cap_material`; a column may omit `height` to reach the local ceiling.
3. An archway's `height` is the whole block, `opening_height` is the clear
   height at the crown and `arch_rise` how much higher the crown is than the
   springing line (`0` is a flat lintel). Centre it on the opening and let its
   ends tuck into the adjoining walls.
4. Every one of them is solid: it blocks the player and occludes the baked
   light.

```json
{ "x": 6.05, "z": 6.6, "width": 1.0, "depth": 0.2, "height": 1.05,
  "material": "home:wall_paint_offwhite_01",
  "cap_material": "home:baseboard_white_01" }
```

```json
{ "x": 5.83, "z": 1.6, "width": 0.34, "depth": 1.4, "height": 3.0,
  "opening_width": 1.0, "opening_height": 2.1, "arch_rise": 0.25,
  "material": "home:wallpaper_offwhite_01",
  "reveal_material": "home:wall_paint_offwhite_01" }
```

## Add a guardrail or handrail

1. `x`, `z` is the start of the run and the rail runs along its own `+X` axis:
   `rotation_degrees` 0 = east, 90 = north, 180 = west, 270 = south.
2. `height` (default 1.0) is the top rail above the base line; `rise` slopes the
   run for a staircase or ramp rail; `post_spacing` (default 1.2) places the
   posts.
3. It is a barrier: it blocks the player and occludes light.

```json
{ "x": 5.35, "z": 2.1, "length": 2.2, "rotation_degrees": 270.0,
  "height": 0.95, "material": "home:handrail_wood_01" }
```

## Add a threshold or a baseboard

1. Both run along their own `+X` axis from their start point (a threshold from
   its centre), rotated the same way as a guardrail.
2. Neither collides: the player walks over a threshold and past a baseboard.
3. A threshold must sit on a level floor; author `length` a couple of
   centimetres wider than the opening so its ends tuck into the jambs.
4. At a corner, stop one board just short of the other's face so no two trim
   faces share a plane.

```json
{ "x": 6.0, "z": 2.3, "length": 1.04, "thickness": 0.08, "height": 0.012,
  "rotation_degrees": 90.0, "material": "home:threshold_wood_01" }
```

```json
{ "x": 0.0, "z": 0.0, "length": 6.0, "height": 0.09, "thickness": 0.018,
  "material": "home:baseboard_wood_01" }
```

## Add a new light fixture

A new *fixture id* always needs a code mesh family (section
[Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)):

1. Add the `FixtureKind` variant, index, profile and id mapping in
   `src/lighting/tuning.rs` (append; never reorder `index()`).
2. Implement the emitter in `src/render/common/fixtures.rs` and dispatch it in
   `src/render/common/geometry.rs::emit_fixtures`.
3. Add a POT opaque face PNG under
   `assets/environment/<theme>/textures/lights/`.
4. Add the `light` catalog entry with `model` = the PNG.
5. Update the family-pinning tests listed in the procedure.
6. If the family needs a pool shape that is not a rectangle, describe it in the
   family's `shape()` (the bake consumes it; no fixture-specific light code needed).
7. Validate: `python3 tools/textures/build.py --check`,
   `python3 tools/assets/validate.py`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`.
8. Place it: `{ "fixture": "<id>", "x": …, "z": … }`
   (plus `mount: "wall"`, `y` for a wall family).

To reuse an existing family with new artwork, only steps 3–4 and 8 are needed, and
the fixture must be the only one claiming that sheet.

## Add a new environment theme

1. Add a `themes` record to `assets/catalog.json`:
   `{ "id": "hotel", "display_name": "Hotel", "description": "…" }`.
2. Create the new theme directory `assets/environment/hotel/` with
   `textures/{walls,floors,ceilings,lights}/`, `props/models/`, `decals/` as
   needed.
3. Register each asset with `"theme": "hotel"` (or leave `theme` off for generic
   assets that do not belong to it).
4. Validate: `python3 tools/assets/validate.py`.
5. Reference hotel assets from any level; themes never restrict placement.

---

# Maintaining This Guide

This document is the contract between the engine and map authors. It must be updated
in the same change whenever the authoring contract changes. Specifically, update it
when adding or changing:

* level fields, collections, defaults, or validation limits;
* geometry types (rooms, walls, profiles, patches, regions);
* opening types or opening behavior;
* texture kinds, formats, size rules or wrapping;
* material properties or resolution behavior;
* asset classes, asset types, catalog fields or catalog validation;
* model formats or the accepted GLB profile;
* prop metadata, placement fields or collision behavior;
* decal placement, surfaces or depth behavior;
* light types, fixture mounting types, or lighting parameters exposed to authors;
* reflections, animated emissions, or any new per-material render behavior;
* validation commands or quality/budget rules.

The structure is deliberately table-based: a new capability should be inserted as a
row or subsection in the appropriate reference section — [Supported Asset Types](#15-supported-asset-types)
for a new asset type, [Current Light Fixture Types](#current-light-fixture-types) for a
new fixture family, [Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)
for the procedure, and the recipe section for a new authoring workflow — rather than
rewriting the document.

When you update this guide:

1. Verify every changed claim against the runtime and tests, not against old prose.
2. Update the metadata block: the format versions and the verification note (say what
   was re-verified and against which tree or build; do not paste a stale SHA without
   re-checking it).
3. Re-run the validation commands in [Validation Workflow](#27-validation-workflow) and
   fix any example that no longer matches.
4. Keep the **Implemented Now** and **Not implemented** lists clearly separated.
   Never document a planned field, type or behavior as authorable, and never invent
   future JSON fields.
5. Keep normal game artwork as external image files in the asset tree; if internal
   generated diagnostics change, update the exceptions table in [Textures](#12-textures).

The metadata block is a verification marker, not a freshness guarantee: a
correct-looking hash does not make a stale claim true. Re-verify against the code.
