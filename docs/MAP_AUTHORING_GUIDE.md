# Places Map Authoring Guide

Repository-wide checks: [authoritative desktop verification](VERIFICATION.md).

| Field | Value |
| --- | --- |
| Document status | **Canonical / living.** Update it whenever the authoring contract changes (see [Maintaining This Guide](#maintaining-this-guide)). |
| Level format version documented | `3` (`format_version` in every level JSON) |
| Asset catalog format version documented | `2` (`format_version` in `assets/catalog.json`) |
| Stage 7 integration audit | October 9, 2026: checked colour/HDR, measured Full ten-page/lower eight-page atlas policy, independent quality controls, panes, PLPF v3/solver 16, geometry revision 7 and compiler/runtime dependency identity against the current source. Wall spans evaluate exact endpoints against their own roof owner. Platform execution claims remain dated evidence in [VERIFICATION.md](VERIFICATION.md). |
| Verification | Sky panorama limits and quality uploads re-verified against `src/assets.rs`, `src/materials/image.rs`, `src/quality.rs` and `src/render/wgpu/sky.rs` (October 2026 working tree). Movement support and rim backing in §10 re-verified against `src/game.rs`, `src/level.rs` and the permanent movement-map tests (October 2026 working tree). Verified against the working tree at version 0.7.0. Section 33 was checked against `src/nav/`, `src/ai/`, `src/package/navigation.rs`, `src/loader.rs`, `src/compiler.rs`, `assets/levels/places_demo.json` and the fixed-step tests `nav::tests`, `ai::tests` and `game::tests::demo_home_encounter_*`. The v3 contract (components, event bindings, trigger volumes, timers, sequences and spawns) was checked against `src/level.rs`, `src/loader.rs`, `src/entities/`, `assets/levels/places_demo.json`, `assets/levels/model_zoo.json`, `levels/*.json` and the generator/converter tools. The `fade`/`glow` components in §29 were checked against `src/level.rs` (`FadeDef`/`GlowDef`), `src/loader.rs` (`validate_component_value`), `src/entities/components.rs` (`Fade`/`Glow`), `src/entity.rs` (`EntityFrame`) and `tools/assets/validate.py`. No commit SHA is pinned: the body was checked line-by-line against `src/level.rs`, `src/loader.rs`, `src/geometry_check.rs`, `src/assets.rs`, `src/materials/`, `src/render/`, `src/lighting/`, `assets/catalog.json`, `assets/levels/places_demo.json` and `tests/fixtures/levels/*.json`. |
| Outdoors reconstruction verification | Section 34 checked against the October 7 Outdoors catalog, fitted PNGs, closed model builders, source placements, native wgpu captures and asset integrity audit. Engine format and lighting model are unchanged. |
| Checks that must pass before a code or asset change ships | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features`; `python3 tools/assets/validate.py`; `python3 tools/textures/build.py --check`; `python3 tools/props/build.py --check` (see [Validation Workflow](#27-validation-workflow) for what each proves) |
| Aero prop authoring verification | October 10, 2026: opt-in BLEND/finite opacity writer checked against importer and byte-preserving OPAQUE/MASK fixtures; existing runtime rendering and transport remain unchanged. |
| Environment refinement verification | October 10, 2026: neighbor-clipped ramp side geometry revision 9 and reusable `weather_alternate`, `weather_cycle`, `toggle_weather`, `set_weather_strength`, `set_weather_cycle`, `set_prompt` checked against current authoring/runtime sources and focused regression tests. Native acceptance and final gates are recorded in [environment-refinement.md](environment-refinement.md). |
| Primary benchmark level | `assets/levels/places_demo.json` |

> This revision describes the **current** engine: the Low/Medium/High quality
> levels, the generic engine-level light model, true emissive materials,
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
   `src/loader.rs` (validation and package discovery), `src/package/` +
   `docs/PACKAGE_FORMAT.md` (the compiled world the player loads),
   `src/compiler.rs` + `src/bin/places-compile.rs` (offline preparation),
   `src/game.rs` / `src/collision.rs` (movement and collision), `src/door.rs`
   (door state and collision pose), `src/render/` (meshes, decals, fixtures,
   props, doors, reflections, post-processing), `src/lighting/` (bake and
   lightmaps), `src/materials/` + `src/assets.rs` (catalog and materials),
   `src/quality.rs` + `src/settings.rs` (quality levels).
2. **Validation and tests** — `src/loader/tests.rs`, `src/level/tests.rs`,
   `src/render/tests.rs`, `src/materials/tests.rs`, `src/assets/tests.rs`,
   `src/props/tests.rs`, `src/collision/tests.rs`, `src/game/tests.rs`,
   `src/lighting/tests.rs`, and the audit modules under `src/` (`surface_audit.rs`,
   `lighting_audit*.rs`, `lighting_isolation.rs`,
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
| Compiled package format | `docs/PACKAGE_FORMAT.md`, `src/package/` |
| Offline compiler | `src/compiler.rs`, `src/bin/places-compile.rs` |
| Collision / walkable floor | `src/collision.rs`, `src/game.rs` |
| Door state / collision pose | `src/door.rs` |
| Door geometry | `src/render/common/doors.rs` |
| Mesh generation | `src/render/common/mod.rs`, `src/render/common/geometry.rs` |
| Decals | `src/render/common/decals.rs`, `src/render/common/mod.rs` |
| Fixture geometry | `src/render/common/fixtures.rs`, `src/lighting/tuning.rs` |
| Prop loading | `src/props.rs`, `src/gltf.rs`, `src/render/common/props.rs` |
| Lighting bake | `src/lighting/` (bake, visibility, occlusion, lightmap) |
| Materials / textures | `src/materials/`, `src/assets.rs` |
| Reflections | `src/render/common/reflections.rs` (routing) and `src/render/wgpu/reflections.rs` (probe cubemaps and the planar target) |
| Post-processing / fog | `src/render/common/postprocess.rs`, `src/render/common/atmosphere.rs`, `src/render/common/framebuffer.rs` |
| Quality levels / settings | `src/quality.rs`, `src/settings.rs` |
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
| A separate dynamic-object render path (per-frame transforms, no rebuild of static geometry or lightmaps) | Implemented (authorable `doors[]` leaves plus one engine-created drum demonstration) |
| Generic engine-level lights (point / rect / line) owned by fixtures and props | Implemented |
| Material emission (`emissive`, `emissive_intensity`, `emissive_mask`) and per-fixture `emission` | Implemented |
| Material surface response (`normal_texture`, `normal_strength`, `specular`, `specular_color`, `shine`) | Implemented |
| Material transparency (`alpha_mode`: `opaque` / `cutout` / `blend`, `opacity`, `alpha_cutoff`) | Implemented |
| Opening glazing: a `glass` material fills a window, vent or door aperture with one pane; `solid: true` makes the pane a physical slab | Implemented |
| Map-wired door actions (`open`, `close`, `toggle`, switchable light fixtures) | Implemented (see [§30](#30-doors-switches-and-effects)) |
| Presentation-only ambient effects (`effects[]`, steam) | Implemented (see [§30](#30-doors-switches-and-effects)) |
| Offscreen scene rendering presented by a fullscreen quad, UI at drawable resolution | Implemented |
| Selective reflections: per-material `reflection_mode` (`none` / `probe` / `planar`); independent Reflections Full/Medium use 64/48-texel probes and a half-resolution planar pass, Off releases both | Implemented |
| Restrained post-processing: emission-driven bloom, a tone shoulder, the global distance fog and a subtle grade, with the UI drawn outside it | Implemented (optional `environment` authors fixed presentation and global atmosphere; regional fog volumes add to it — see [§11](#fog-a-global-atmosphere-plus-level-authored-regions)) |
| Level-authored regional fog volumes (`fog_regions[]`, ≤ 16) | Implemented (see [§11](#fog-a-global-atmosphere-plus-level-authored-regions)) |
| Opaque void wall / floor boxes (`void_walls[]`, ≤ 256) for hiding the void | Implemented (see [§11](#void-wall-and-floor-boxes)) |
| Animated emissions: `animated_emissions[]` makes a material's emission pulse or flicker, deterministically | Implemented |
| Low / Medium / High runtime quality levels over the same level content | Implemented |
| Props/entities from GLBs by logical id, `solid` collision boxes | Implemented |
| Multi-primitive / multi-material GLB props, embedded emissive materials, node transforms | Implemented |
| External PNG surfaces, decals, fixture faces; catalog + themes | Implemented |
| Compiled map packages (`.placesmap`) with prepared geometry, lightmaps, collision and reflection probes | Implemented (see [PACKAGE_FORMAT.md](PACKAGE_FORMAT.md)) |
| Door leaves (`doors[]`): interior and sauna kinds, state machine, obstruction handling, locked state, component/binding-driven interaction | Implemented (see [§30](#30-doors-switches-and-effects)) |
| Water volumes (`water[]`): a translucent surface, wading, swimming and surface swimming | Implemented (see [Water volumes](#water-volumes-wading-swimming-and-surfacing)) |
| Ladder volumes (`ladders[]`): walking into the face climbs without a key, with release, backing away, jump, obstruction and top-landing rules | Implemented (see [Ladders](#ladders)) |
| Stable per-instance ids, typed components, event bindings, map-authored interactions (E), floating labels, reset-to-start | Implemented (see [§29](#29-entities-components-bindings-volumes-timers-sequences-and-spawns)) |
| Trigger volumes (`volumes[]`): enter/exit semantics, swept fast-fall crossings, cooldowns, `once`, typed action batches | Implemented (see [§29](#29-entities-components-bindings-volumes-timers-sequences-and-spawns)) |
| Timers, sequences, spawn templates/points/groups (including at-most-one-active) | Implemented (see [§29](#29-entities-components-bindings-volumes-timers-sequences-and-spawns)) |
| Baked navigation (`nav_agent` bodies, per-class clearance, stairs/slopes, door portals), runtime path queries and the shared AI framework (`ai`: idle/wander/follow/flee/investigate/pursue/catch, sight and hearing, catch -> sequence) | Implemented (see [§33](#33-navigation-and-ai)); the player loads the compiler's bake and never builds one. |
| Animation actions (`play_animation`, `toggle_animation`) | Implemented per instance; the target must carry an `animation` component. |
| Audio actions (`play_sound`, `stop_sound`) | Implemented as a typed emitter state on an entity with an `audio` component; no audio device backend exists in this tree, so a playback request is reported once instead of playing. No shipped map authors one. |
| Water refraction, realtime shadow maps and runtime static transport solves | Not implemented; straight alpha/depth transmission and bounded entity direct lights are implemented |
| Animated entities: a placed skinned GLB follows the player's locomotion state (idle/walking/airborne/swimming); a rig with authored clips plays them, a no-clip rig uses the built-in procedural gait | Implemented (see [§16](#16-props-and-models)) |
| Screen-space reflections; per-frame raytraced reflections; cubemap probes with realtime updates | Not implemented (static probes and one planar plane exist) |
| GLB MASK/cutout and BLEND on static props, characters and dynamic props | Implemented; shared back-to-front object/batch sorting, without intersecting-triangle sorting |
| Emissive decals; per-placement emission overrides; cone/spot lights | Not implemented |
| Authoring a normal map from a level (a level names a material, and the material owns the map) | Implemented (via the catalog) |
| Sloped floors (`ramps`), staircases (`stairs`), half walls, columns, archways, guardrails, thresholds, baseboards | Implemented |
| Data-authored arc (curved) walls (`arc_walls[]`) and circular pillars (`pillars[]`), with per-primitive tessellation, per-face materials and segment-derived collision | Implemented (see [§10](#10-floors-elevation-and-vertical-geometry)) |
| A read-only geometry checker CLI (`--check-geometry`) over the engine's own authored/generated geometry | Implemented (see [§32](#31-the-map-geometry-checker)) |
| Per-room ceiling tile frame (`ceiling_tile_origin`, `ceiling_tile_rotation_degrees`) for offset/rotated ceiling patterns and decal snapping | Implemented |
| Decal grid snapping (`decals[].align: "ceiling_grid"`) onto the ceiling's own panel module | Implemented |
| Narrow intent annotations for the checker (`geometry_intent[]`) | Implemented (see [§32](#31-the-map-geometry-checker)) |
| Ceiling/floor openings; traversal between stacked storeys | Not implemented (a stair or ramp between floors is; overlapping walkable surfaces at one `(x, z)` are not) |
| Runtime navmesh editing, off-mesh links, jumping/climbing agents, swimming agents, dynamic obstacle avoidance beyond door leaves | Not implemented |
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
17. **Compile the level.** `./target/release/places-compile build <source>.json`
    (or `cargo run --release --bin places-compile -- build <source>.json`). The
    compiler validates the source, prepares the static world and publishes the
    `.placesmap` beside it. A failed build leaves any previous package untouched.
    Then `cargo test --workspace --all-features`, and boot with
    `PLACES_LEVEL=<id> cargo run`. A package that fails to open is reported as
    `[levels] skipping …` at discovery, so read the console even when the level is
    meant to appear in the menu.
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
doorway) → reuse `core:carpet_damp_01` for flood-damaged surfaces, put a
`water[]` volume over the recessed basement floor for the standing water (see
[Water volumes](#water-volumes-wading-swimming-and-surfacing)), and keep damp
materials for everywhere the water does not reach → dim green lights as ordinary
ceiling fixtures with
`"color": [0.35, 1.0, 0.45]` and low `brightness` → validate. Do **not** author a
second variant of the level for a lower quality level; one level serves all
three.

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
tables of sections 6–10, 16, 17 and 19–21, plus doors, switches and effects in
[§30](#30-doors-switches-and-effects), and the skeleton below names every current
field at least once. Unknown keys
are **silently ignored** (the structs do not use
`deny_unknown_fields`), so a typo disappears without an error; diff against the
skeleton and the per-field tables.

```jsonc
{
  "format_version": 3,                     // REQUIRED. Must be exactly 3.
  "id": "my_level",                        // REQUIRED. Non-empty; menu key.
  "name": "My Level",                      // REQUIRED. Non-empty; display name.
  "author": "",                            // optional, default "".

  "sky": {                                 // optional; a data-driven night-sky background
    "texture": "outdoor:tex_sky_stars_01", // a catalog `texture` asset, equirectangular 2:1
    "brightness": 1.0,                     // optional, default 1.0; 0.0..4.0
    "ambient": 0.0                         // optional, default 0.0; 0.0..1.0
  },                                       // omitted = no background object (clear colour)

  "fog_regions": [                         // optional; regional fog boxes, ≤ 16, see §11
    { "id": "yard_mist",                   // REQUIRED, unique, ≤ 64 chars
      "min": [-2.0, -2.0, -6.0],           // REQUIRED, finite, < max on every axis
      "max": [8.0, 1.0, 2.0],
      "density": 0.05,                     // REQUIRED, 0.0..=0.5 per metre
      "color": [0.6, 0.63, 0.68],          // optional; default = the global fog colour
      "falloff_m": 3.0,                    // optional; ≥ 0, default 2.0; soft edge inside
      "ground_y": -2.0,                    // optional; default min.y
      "top_y": 1.0 }                       // optional; default max.y (density fades to 0)
  ],

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
      "ceiling_shine": 0.0,                // optional 0..1; default = material's own
      "ceiling_tile_origin": [0.75, 0.25], // optional local phase of the ceiling tile pattern (world x/z)
      "ceiling_tile_rotation_degrees": 90.0 // optional rotation of the ceiling tile pattern
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
          "glass_shine": 0.2,              // optional 0..1 for the pane
          "solid": true                    // optional, default false; true = the pane blocks the player (requires glass)
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

  "water": [                               // translucent swimming volumes; no geometry
    {
      "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0, // REQUIRED footprint, > 0
      "surface_y": -1.65,                  // REQUIRED world Y of the free surface
      "bottom_y": -3.0,                    // optional; default: the lowest floor below
      "material": "core:water_pool_01",    // optional; default core:water_pool_01
      "opacity": 0.62,                     // optional 0..1; default 0.62
      "swimming": true                     // optional; false keeps it decorative
    }
  ],

  "ladders": [                             // climbable volumes; the prop draws the rails
    {
      "x": 19.35, "z": 11.7, "width": 0.6, "depth": 0.6, // REQUIRED footprint, > 0
      "bottom_y": -3.0,                    // REQUIRED lowest climbable world Y
      "top_y": -1.5,                       // REQUIRED exit world Y, above bottom_y
      "facing_degrees": 90.0               // optional; climb yaw, default 0 (towards -Z)
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

  "void_walls": [                          // optional; opaque void-hiding boxes, ≤ 256, see §11
    { "id": "yard_shell_north",            // optional, unique when present, ≤ 64 chars
      "min": [-2.0, -0.2, -92.0],          // REQUIRED finite box, min < max
      "max": [26.0, 9.0, 0.6],
      "material": "outdoor:dirt_gravel_01",// REQUIRED catalog material id
      "faces": "inward",                   // "inward" (default) | "outward" | "both"
      "solid": true,                       // optional; default true: the box collides
      "occludes": true }                   // optional; default true: bakes as an occluder
  ],

  "arc_walls": [                           // curved wall slabs, placed by circle centre
    { "x": 14.0, "z": 4.0, "radius": 1.0,   // centreline radius
      "thickness": 0.24,                    // ring thickness (default 0.3)
      "height": 2.2,                        // omitted follows the local ceiling
      "start_degrees": 180.0, "sweep_degrees": 180.0, // compass angles, + = N→E→S→W
      "segments": 16,                       // default: 24 per full circle, scaled to the sweep
      "material": "core:wallpaper_stained_01",
      "inner_material": "core:pool_tile_wall_01", // optional concave face
      "cap_material": "core:baseboard_office_01"  // optional top/bottom caps
      // end_material overrides the two radial ends; any catalog material is valid
    }
  ],

  "pillars": [                             // solid circular pillars, placed by centre
    { "x": 3.5, "z": 8.5, "radius": 0.4,
      "height": 3.0,                        // omitted follows the local ceiling
      "segments": 24,                       // default 24
      "material": "core:pool_tile_wall_01",
      "cap_material": "core:baseboard_office_01" }
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
      "align": "ceiling_grid",             // optional; snap a ceiling decal to the ceiling's panel grid
      "surface": "floor" }                 // REQUIRED enum
  ],

  "ceiling_lights": [                      // ALL fixtures, ceiling and wall
    {
      "fixture": "core:pool_light_wall",   // REQUIRED
      "x": 0.15, "z": 13.0,                // REQUIRED
      "rotation_degrees": 90.0,            // optional, default 0.0
      "brightness": 0.7,                   // optional; default 1.0
      "color": [0.55, 0.78, 1.0],          // optional; default [1.0, 0.96, 0.88]
      "mount": "wall",                     // optional; default "ceiling"
      "y": 1.9,                            // REQUIRED when mount is "wall"
      "range": 6.0,                        // optional, default 6.0
      "falloff": "smooth",                 // optional, default "smooth"
      "enabled": true,                     // optional, default true
      "emission": 0.7,                     // optional; default = brightness
      "switchable": true,                  // optional, default false; a `toggle` action may drive it
      "align": "grid"                      // optional; "grid" (default) or "none"
    }
  ],

  "props": [
    {
      "id": "front_desk",                 // optional; stable per-instance id (default <model>_<n>)
      "display_name": "Front Desk",       // optional; label text for toggle_label (default model id)
      "model": "core:desk",                // REQUIRED
      "x": 2.0, "y": 0.0, "z": 5.0,        // optional, default 0.0; y is floor-relative
      "rotation_degrees": 0.0,             // optional, default 0.0
      "scale": 1.0,                        // optional, default 1.0, must be > 0
      "size": [1.6, 0.75, 0.7],            // optional [w,h,d]; collision box, x scale
      "solid": true,                       // optional, default false
      "occludes": true,                    // optional, default true; false for alpha-cutout scenery (grass)
      "components": [                      // optional typed capabilities; see §29
        { "component": "interactable", "prompt": "Toggle name", "reach": 2.5 },
        { "component": "state", "name": "phase", "value": "cold" },
        { "component": "animation", "clip": "toggle", "looped": false, "playing": false },
        { "component": "light", "enabled": true, "switchable": false, "emission_scale": 1.0 },
        { "component": "material", "variants": [ { "name": "on", "emission_scale": 1.0 } ] }
      ],
      "bindings": [                        // optional event wiring; see §29
        {
          "on": "interact",                // closed event kind enum
          "key": null,                     // optional event key filter
          "when": [ { "check": "state", "target": "front_desk", "name": "phase", "equals": "cold" } ],
          "once": false,                   // optional, default false
          "cooldown_seconds": 0.0,         // optional, default 0.0; >= 0
          "actions": [                     // REQUIRED, 1..8 actions, in order
            { "action": "toggle_label" }
          ]
        }
      ],
      "lights": [                          // optional, default []; max 8 per prop
        {
          "shape": "rect",                 // optional; default "point"
          "half_width": 0.3, "half_depth": 0.05,  // used by "rect"
          "length": 1.2,                   // used by "line"
          "offset": [0.0, 0.9, 0.3],       // optional, default [0, 0, 0]
          "rotation_degrees": 0.0,         // optional, default 0.0
          "color": [0.55, 0.78, 1.0],      // optional; default [1.0, 0.96, 0.88]
          "intensity": 0.15,               // optional; default 1.0
          "range": 3.0,                    // optional, default 6.0
          "falloff": "smooth",             // optional, default "smooth"
          "enabled": true                  // optional, default true
        }
      ]
    }
  ],

  "doors": [                               // optional; interactive leaves, see §30
    {
      "id": "hall_door",                   // REQUIRED; unique across props/lights/doors/volumes/timers/points
      "x": 60.3, "y": 0.0, "z": 3.0,       // hinge edge; y is above the walkable floor
      "rotation_degrees": 0.0,             // yaw of the closed leaf; 0 runs toward +X, 90 toward -Z
      "width": 1.4, "height": 2.1,         // REQUIRED; leaf size in metres
      "thickness": 0.045,                  // optional; default 0.045
      "open_direction": "left",            // optional; "left" (default) or "right"
      "swing_degrees": 90.0,               // optional; default 90, 5..179
      "open_speed_degrees": 120.0,         // optional; default 120, 0..720
      "close_speed_degrees": null,         // optional; default = open speed
      "initial_state": "closed",           // optional; "closed" (default) or "open"
      "locked": false,                     // optional; default false (true refuses to open until unlocked)
      "components": [                      // a manual door authors an interactable component
        { "component": "interactable", "prompt": "Hall door" }
      ],
      "bindings": [                        // and its own `interact` binding
        { "on": "interact", "actions": [ { "action": "toggle" } ] }
      ],
      "obstruction": "stop",               // optional; "stop" (default) or "reverse"
      "kind": "interior",                  // optional; "interior" (default) or "sauna"
      "material": null, "frame_material": null, "handle_material": null  // optional overrides
    }
  ],

  "effects": [                             // optional; presentation-only emitters, see §30
    {
      "id": "sauna_steam_a",               // optional; diagnostics only
      "kind": "steam",                     // REQUIRED; only "steam"
      "x": 31.0, "y": 0.0, "z": 12.6,      // optional; y is above the walkable floor
      "width": 0.9, "depth": 0.6,          // optional footprint, default 0.8 each
      "height": 1.5,                       // optional plume height, default 1.6
      "count": 20,                         // optional particle count, default 24, <= 128
      "size": 0.34,                        // optional billboard size, default 0.35
      "drift": 0.16,                       // optional horizontal wander, default 0.0
      "lifetime_seconds": 3.2,             // optional, default 3.0, <= 60
      "material": null,                    // optional; default core:steam_01
      "enabled": true,                     // optional, default true
      "bindings": []                       // optional; the emitter's own events
    }
  ],

  "volumes": [                             // optional; enter/exit trigger volumes, see §29
    {
      "id": "pit_hole_1",                  // optional; default trigger_<n> (1-based)
      "x": 9.6, "z": -26.2,                // optional; footprint MIN corner, default 0
      "width": 1.6, "depth": 1.6,          // REQUIRED, > 0
      "bottom_y": -3.2,                    // optional; default floor under the centre
      "top_y": -0.05,                      // optional; default bottom_y + 2.0
      "bindings": [                        // REQUIRED in practice, 1..16 bindings
        { "on": "enter_volume", "once": false, "cooldown_seconds": 0.5,
          "actions": [ { "action": "reset_to_start" } ] }
      ]
    }
  ],

  "timers": [                              // optional; headless timer entities, see §29
    { "id": "sauna_warmup_timer", "seconds": 1.5, "repeat": false, "autostart": false,
      "bindings": [ { "on": "timer", "actions": [ { "action": "start_sequence",
                       "sequence": "sauna_warmup", "target": "sauna_door" } ] } ] }
  ],

  "sequences": [                           // optional; ordered steps on one entity, see §29
    { "id": "sauna_warmup", "looped": false, "steps": [
        { "step": "wait", "seconds": 0.5 },
        { "step": "set_state", "name": "phase", "value": "warm" },
        { "step": "emit", "on": "timer", "key": "warm" }
      ] }
  ],

  "spawn_templates": [                     // optional; typed prefabs, see §29
    { "id": "crate_spawn", "model": "core:crate", "scale": 0.5,
      "lifetime_seconds": 20.0,
      "components": [ { "component": "state", "name": "phase", "value": "spawned" } ],
      "bindings": [] }
  ],

  "spawn_points": [                        // optional; where a template appears
    { "id": "crate_spawn_point", "x": 12.0, "z": 16.3, "yaw_degrees": 0.0,
      "template": "crate_spawn", "group": "crate_group", "bindings": [] }
  ],

  "spawn_groups": [                        // optional; at-most-one-active bookkeeping
    { "id": "crate_group", "at_most_one_active": true }
  ],

  "animated_emissions": [
    {
      "material": "core:glass_sign_lit_01",  // REQUIRED
      "effect": "pulse",                   // optional; `pulse` or `flicker`; default pulse
      "hz": 0.09,                          // optional; effect default when absent
      "depth": 0.18,                       // optional; effect default when absent
      "phase": 0.0                         // optional, default 0.0
    }
  ],

  "geometry_intent": [                     // optional; narrow checker annotations
    { "check": "missing-wall",             // optional check id; omitted covers every heuristic check
      "x": 11.8, "z": 11.6, "width": 4.4, "depth": 0.8,   // plan rectangle, min corner
      "note": "Intended open route between the two spaces." }
  ]
}
```

**Warning — the `defaults` gotcha.** If the `defaults` key is absent entirely, the
engine uses `core:wallpaper_yellow_01` / `core:carpet_beige_01` /
`core:ceiling_panel_01`. If you author `defaults`, author **all three keys**: each
missing key becomes the empty string, and an empty material id resolves to the
*untextured white sheet*, not the built-in default. Never leave a material id empty.

**Closed enums vs free strings.** Serde rejects the whole document at parse time when a
*closed enum* field has an unknown value: `ceiling.kind`, `ridge`, `mount`, `falloff`,
`shape` (prop light), door `open_direction` / `initial_state` / `obstruction` / `kind`,
and decal `surface`. Free strings are validated later by the
loader or the renderer: opening `kind` (unknown names load), animated-emission
`effect` (unknown names are a named validation error), and all logical asset ids.
A misspelled enum is a parse error, not a silently ignored key.

**Where levels live.** Players load compiled `.placesmap` packages only.
`assets/levels/*.placesmap` ships with the game; `levels/*.placesmap` are drop-in
packages (under the writable state root, normally next to the asset root). Both appear
in the Level Select menu. The `.json` sources beside the bundled packages are kept for
authors and are never playable rows; discovery enumerates playable packages only and
skips sources silently, so the normal startup log stays clean. A raw source is only
reported when you explicitly try to open or import it: that fails with the compiler
command (`places-compile build <source.json>`), because compiling is always an
offline, author-side step. `tests/fixtures/levels/` is for engine regression fixtures
and is never packaged.
A package that fails to open or validate is reported at discovery as
`[levels] skipping {path}: {reason}`, so check the console rather than assuming it is
absent. `PLACES_LEVEL=<id>` boots a specific level and prints validation errors
verbatim.

### Level limits

The engine enforces several independent caps. Only some of them reject a level:
read the "Enforced as" column carefully. The 2026 capacity pass raised every
count cap after measuring the extended fixture
(`tests/fixtures/levels/capacity_beyond_former_limits.json`: 20 001 props,
20 001 walls, 2 025 rooms, more than 80 000 distinct material ids past the
former 16-bit index boundary) and the maintained dense fixture
(`tests/fixtures/levels/capacity_dense.json`, 5 000+ placements, 100+ fixtures,
18 routed entities) on the release build. The sparse source was retired
earlier because the package compiler cannot represent its navigation grid;
CPU coordinate regressions remain in `src/zoo_audit.rs`; each value is a named
constant in `src/level.rs`, not an inline literal.

| Limit | Value | Enforced as |
| --- | --- | --- |
| Rooms (`rooms`) | ≤ 8000 (`MAX_LEVEL_ROOMS`) | Loader rejection: `Level contains too many rooms: …` |
| Walls | ≤ 60 000 (`MAX_LEVEL_WALLS`) | Loader rejection |
| Ceiling lights | ≤ 50 000 (`MAX_LEVEL_CEILING_LIGHTS`) | Loader rejection |
| Props | ≤ 100 000 (`MAX_LEVEL_PROPS`) | Loader rejection |
| Decals | ≤ 20 000 (`MAX_LEVEL_DECALS`) | Loader rejection |
| Decal edge (`width`, `height`) | ≤ 10 m | Loader rejection |
| Floor regions | ≤ 8000 | Loader rejection |
| Water volumes | ≤ 8000 | Loader rejection |
| Ladders | ≤ 1024 | Loader rejection |
| Doors (`doors[]`) | ≤ 24 (`MAX_LEVEL_DOORS`) | Loader rejection; two dynamic objects per door share the renderer budget |
| Door swing | 5–179° (`MIN_DOOR_SWING_DEGREES`, `MAX_DOOR_SWING_DEGREES`) | Loader rejection per door |
| Door angular speed | 0 < speed ≤ 720°/s (`MAX_DOOR_SPEED_DEGREES`) | Loader rejection per door |
| Door width/height/thickness | each ≤ 12 m (`MAX_DOOR_DIMENSION_M`) | Loader rejection per door |
| Effects (`effects[]`) | ≤ 64 (`MAX_LEVEL_EFFECTS`) | Loader rejection |
| Effect particles (`count`) | 1–128 (`MAX_EFFECT_PARTICLES`) | Loader rejection per effect |
| Trigger volumes (`volumes`) | ≤ 4000 (`MAX_LEVEL_AREA_TRIGGERS`) | Loader rejection |
| Bindings per entity | ≤ 16 (`MAX_BINDINGS_PER_ENTITY`) | Loader rejection |
| Actions per event binding or sequence step | ≤ 8 (`MAX_ACTIONS_PER_SOURCE`) | Loader rejection |
| Authored interactable `reach` | ≤ 4.0 m | Loader rejection |
| Timers | one entity per entry; `seconds` > 0 | Loader rejection per timer |
| Sequences (`sequences`) | ≤ 1024 (`MAX_LEVEL_SEQUENCES`) | Loader rejection |
| Steps per sequence | ≤ 128 (`MAX_SEQUENCE_STEPS`) | Loader rejection per sequence |
| Spawn templates | ≤ 256 (`MAX_LEVEL_SPAWN_TEMPLATES`) | Loader rejection |
| Spawn points | ≤ 1024 (`MAX_LEVEL_SPAWN_POINTS`) | Loader rejection |
| Spawn groups | ≤ 256 (`MAX_LEVEL_SPAWN_GROUPS`) | Loader rejection |
| Ramps | ≤ 2000 | Loader rejection |
| Staircases | ≤ 2000 | Loader rejection |
| Half walls | ≤ 8000 | Loader rejection |
| Columns | ≤ 8000 | Loader rejection |
| Arc walls | ≤ 4000 | Loader rejection |
| Circular pillars | ≤ 8000 | Loader rejection |
| Round-primitive `segments` | 3–128 | Loader rejection per primitive |
| Archways | ≤ 2000 | Loader rejection |
| Guardrails | ≤ 8000 | Loader rejection |
| Thresholds | ≤ 4000 | Loader rejection |
| Baseboards | ≤ 8000 | Loader rejection |
| Floor patches | ≤ 8000 | Loader rejection |
| Distinct materials (`material` ids the level references) | ≤ 131 072 (`MAX_LEVEL_MATERIALS`) | Loader rejection: `Level declares too many distinct materials: …` |
| Fog regions (`fog_regions`) | ≤ 16 (`MAX_FOG_REGIONS`) | Loader rejection; the shader's uniform array is fixed at 16 and the quality preset uploads a prefix |
| Void walls (`void_walls`) | ≤ 256 (`MAX_VOID_WALLS`) | Loader rejection |
| Openings per wall | ≤ 64 | Loader rejection on that wall |
| Room width/depth | ≤ 8192 m (`MAX_ROOM_EXTENT_M`) | Loader rejection per room |
| Room height | ≤ 50 m (`MAX_ROOM_HEIGHT_M`) | Loader rejection per room |
| Gable `ridge_rise` | ≤ 50 m, and > 0 | Loader rejection per room |
| Estimated floor area | ≤ 64 000 000 m² (`MAX_LEVEL_FLOOR_AREA_M2`) | Loader rejection (its own estimate, computed from room rectangles) |
| Estimated generated vertices | ≤ 24 000 000 (`MAX_LEVEL_VERTICES`) | Loader rejection (its own upper-bound estimate) |
| Distinct decoded texture bytes | ≤ 1 GiB (`MAX_LEVEL_TEXTURE_BYTES`) | Loader rejection per level, after resolution and before upload |
| Standalone level JSON file size | ≤ 32 MiB (`MAX_LEVEL_JSON_BYTES`) | Rejected before parsing; the embedded fallback demo is exempt |
| Package archive: entries / entry / aggregate | ≤ 512 entries / ordinary entry ≤ 256 MiB; mesh/props ≤ 512 MiB, atlas ≤ 320 MiB + 64 KiB, other typed records may be tighter / aggregate ≤ 1 GiB uncompressed | Package rejected at open (see [PACKAGE_FORMAT.md](PACKAGE_FORMAT.md)) |
| Package variants | one per lightmap quality (`off`/`medium`/`full`) | Package rejected at open |
| Package blob hashes | SHA-256 must match the name and the manifest | Blob rejected before decoding |
| Distinct prop models placed | ≤ 4096 (`MAX_LEVEL_PROP_MODELS`) | **Not a rejection:** later placements draw placeholder boxes |
| Summed prop vertices (after instancing) | ≤ 24 000 000 (`MAX_LEVEL_PROP_VERTICES`) | **Not a rejection:** further placements draw placeholder boxes |

Doors stay at 24 by derivation, not by accident: a door draws its frame and its
leaf as two dynamic objects, one level's dynamic scene is bounded by
`MAX_DYNAMIC_OBJECTS` (64), and the level's other dynamic content (floating
props, the demonstration drum) needs slots in the same budget. Two objects per
door over the cap would starve those, so 24 is the real renderer ceiling.

Distinct materials are counted exactly as the renderer binds them: the loader
counts the id set `crate::materials::referenced_material_ids` resolves and
refuses a larger count by name *before* any image is decoded or uploaded.
That count is the only bound on the material index: the index itself is 32 bits
wide (it was a `u16`, which silently collapsed every material past 65 535 onto
the "no material" sentinel).

The prop-model caps (triangles, vertices, primitives, materials, images, texture edge)
are listed in [Props and Models](#16-props-and-models); an over-budget *model* falls
back to a placeholder box with a one-time `[props]` warning.

**Map capacity versus GPU residency.** The count caps above bound what a
*package may declare*: authored data. What the renderer uploads at once is
bounded separately, and the two are deliberately different numbers:

* the level's distinct decoded texture set is bounded by
  `MAX_LEVEL_TEXTURE_BYTES` (1 GiB) per level;
* the lightmap atlas is bounded by the page budget in
  [PACKAGE_FORMAT.md](PACKAGE_FORMAT.md);
* dynamic objects, characters and probes each have their own budget.

A level can therefore be *well-formed* and still too large in one resident
budget; that budget names itself when it refuses.

**Measured behaviour at the raised limits.** The extended capacity fixture
(`capacity_beyond_former_limits.json`) compiles offline in 147 s at the `off`
lightmap profile on four workers (`--workers 4`, peak RSS 856 MiB), producing a
45.4 MiB package whose mesh, props, baked lighting, collision and navigation
records decode through the package reader in 5.8 s (peak RSS 481 MiB). The
compiled world carries 78 839 drawable ranges and 1 032 715 vertices, and its
recorded material index set crosses the former 16-bit boundary. The dense
capacity fixture loads cold in ~45 s at Low-lightmap profile (about 19 s of
prop expansion and 20 s of lightmap fill) and warm in ~24 s, and the level runs
at interactive frame rates. The per-frame cost of collision, support, headroom,
aiming and route movement no longer scales with the wall count: those queries
go through the collision index (`src/collision_index.rs`), and `zoo_audit`
pins the indexed result equal to the linear scan over the real fixtures. Notes
that remain load-time costs: the lightmap fill is linear in chart texels ×
nearby fixtures, and the prop vertex expansion samples lighting per vertex. See
`docs/reports/feature-expansion-summary.md` for the historical measurements.

---

## 6. Level Metadata and Spawn

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `format_version` | integer | **yes** | — | Must be `3`; anything else is rejected: `Unsupported level format_version: {v} (expected 3)`. |
| `id` | string | **yes** | — | Non-empty after trim. Not checked for uniqueness across files (see caveats). |
| `name` | string | **yes** | — | Non-empty after trim. |
| `author` | string | no | `""` | Display metadata only. |
| `spawn.x` | number | **yes** | — | World X. Must be finite. |
| `spawn.z` | number | **yes** | — | World Z. Must be finite. |
| `spawn.yaw_degrees` | number | no | `0.0` | 0 = north (−Z), +90 = east (+X). Must be finite when present. |

Known-valid header (from Places Demo):

```json
{
  "format_version": 3,
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
| `width` | number | **yes** | — | X extent, `> 0`, `≤ 8192` m (`MAX_ROOM_EXTENT_M`). |
| `depth` | number | **yes** | — | Z extent, `> 0`, `≤ 8192` m (`MAX_ROOM_EXTENT_M`). |
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

An **open** ceiling — no ceiling surface at all, so a level's `sky` shows above
an exterior. Floors, walls and contents are emitted exactly like a flat room and
`height` remains the volume's eave height (walls and fixtures resolve against
it); the walkable-ceiling model skips the room, so nothing clamps a jumping
player:

```json
{ "kind": "open" }
```

* Tagged enum: `kind` is `"flat"`, `"gable"` or `"open"`; an unknown `kind` is a JSON parse error.
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

* **Geometry and the point-based authoring height queries** use the **first room in resolution
  order** (`rooms` order) whose footprint contains the
  point (0.01 m tolerance). If several rooms overlap, the earlier one wins.
* **Baked lighting** uses the **smallest-area** room at that point when no height hint
  is authored; an authored light `y` first picks the room whose vertical air volume
  contains that height, so a fixture on one storey does not lend its power to the
  other. Ties break to the smaller area, then to the earlier room.

Both floors/ceilings of an intentional overlap are emitted. This mismatch is a known
design property, not a bug; keep overlapping footprints deliberate and minimal.

The player controller considers all emitted floors and ceilings in the live
body's height band. Support selects the highest reachable surface beneath the
feet; a lower storey's ceiling cannot become an overhead limit for a player
above it. These contact queries do not change rendering ownership or fixture
placement.

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
Wall profiles, visible faces, caps and collision extents share roof ownership:
a containing closed room retains first-room precedence; an outside centre plane
can use the closed roof that physically touches its wall footprint at that length
position. An adjoining open volume does not replace that roof. Room transitions
and every owning gable ridge split the wall into linear spans. Outer corner ends
may continue a contacting roof only over the wall's own thickness, with a floating
point rounding bound; internal gaps cannot borrow a nearby roof. Point-based
floor/ceiling authoring queries retain the overlap rule above. Explicit heights
remain appropriate for freestanding barriers or deliberate rigid silhouettes.
With `height` authored, the wall is drawn exactly `y..y+height`, even through a
ceiling or across rooms. Validation requires a positive finite `height` when it is
present.

An explicit-height wall keeps its exposed horizontal top. Only actual coplanar
closed flat-roof, floor or earlier-wall coverage removes those cap rectangles;
a touching ceiling owner alone does not hide a cap outside the roof footprint.

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
  The geometry checker's `duplicate-surface`, `coplanar-sliver` and
  `prop-layer-coplanar` checks cover emitted architecture and prop dressing;
  still inspect junctions manually and keep one physical wall per surface.
* Decorative kit layers (sidewalk kits, curbs, porch decks) must not rely on a
  millimetre offset under a raised floor region: a buried layer that close can
  z-fight or darken the floor above it, and a top a millimetre proud can cut a
  visible stripe across the floor. Either keep the region as the only surface
  and bury the kit a full layer below the surrounding ground plane, or raise
  the kit clear of the region and split the floor regions to match its real
  profile (the Lantern Hollow porch uses two regions, platform and tread).

---

## 9. Openings

An opening is a rectangular hole cut through a wall's thickness. All opening kinds
are the same rectangle; `kind` only changes labels and one lighting behavior.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `kind` | string | no | `"door"` | Free string; documented spellings `door`, `window`, `passage`, `vent`. Unknown strings load but are not doors for lighting. |
| `offset` | number | **yes** | — | Distance along the wall's length axis from the min corner to the opening's near edge; `≥ 0`. |
| `width` | number | **yes** | — | Cut width along the wall; `> 0`; `offset + width ≤ length` (tolerance 1e-3) or the level is rejected. |
| `height` | number | **yes** | — | Cut height above the sill; `> 0`. |
| `sill` | number | no | `0.0` | Bottom edge above the wall's base (`wall.y`); `≥ 0`. `0.0` reaches the floor. |
| `glass` | string | no | — | Material id of a pane filling the aperture. Absent (or blank) = a bare hole. See [Panes](#panes-glass-grilles-and-screens). |
| `glass_shine` | number | no | material default | Per-surface glossiness (`0`–`1`) for the `glass` pane. |
| `solid` | boolean | no | `false` | Whether the glazed opening **physically blocks the player**. `true` requires `glass` (an invisible solid barrier is a wall, not an opening) and adds a thin collision slab at the pane's plane; `false` is a purely visual pane. Rendering transparency and collision are independent: the transparent blend pass never decides whether the pane stops a body. |

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

A walk-through `door` or `passage` opening is also the aperture an interactive
`doors[]` leaf fills;
size the opening for the leaf and place the leaf by its hinge
(see [§30](#30-doors-switches-and-effects)).

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
a window you can look *through* rather than *into*. A shipped window also authors
`"solid": true`, so the pane blocks the player as a physical slab while still
drawing transparently:

```json
{ "kind": "window", "offset": 1.65, "width": 3.2, "height": 1.3, "sill": 1.7,
  "glass": "core:glass_window_dirty_01", "solid": true }
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
  above foot height blocks. A glazed opening that authors `"solid": true` adds
  the pane's own thin slab (60 mm) back into collision, so a sealed window or
  glass wall blocks the whole opening while a `solid: false` pane stays
  walk-through.
* **A window's interior must be authored.** Whether glazed or open, an aperture
  looks through to whatever geometry is behind it; there is no automatic
  backing.
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
material wins over patches. Patches are counted against the 8000-patch cap, but
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
* Do not cover a basin skirt with a coincident divider face. When the skirt already
  closes the divider below the surrounding deck, start the divider at deck height;
  extending it to the basin bottom duplicates the visible face and can flicker.
  `water_transmission.json` exercises this boundary and its geometry regression.
* `offset_y: 0` is a legal region that only changes material (like a patch) and
  emits no skirt.

### Frozen water and ice traction

A material definition may author `"ground_surface": "ice"`; the default is
`"normal"`. This is a physical material property, separate from `shine`,
alpha and reflections. It applies to the supporting room floor, latest patch,
region, ramp or stair material, using the same precedence as rendered surfaces.
Solid props, bridges and another storey above ice retain normal traction.

Ice approaches requested walk velocity at 4.5/s, coasts with 1.6/s drag and
retains ordinary maximum walk speed. Direction changes and braking take time;
normal terrain immediately resumes ordinary input-driven walking. Ice takeoff
retains planar momentum in air with restrained steering (1.2/s), and landing
uses the contacted surface. Jump height, gravity, step reach, head clearance,
rim and swept collision are unchanged. Reset and ladder attachment clear ice
momentum. Blocked movement spends blocked momentum.

For a winter variant of a pond, remove its `water[]` volume and author a solid
`floor_regions[]` ice floor at the former waterline, with an offset relative to
the containing room. Keep shoreline rises within the 0.4 m step allowance or
provide a snow ramp. Merely setting `swimming: false` leaves a non-solid water
surface and does not freeze it. Never leave a liquid swim volume intersecting
an ice floor. Non-winter sources retain their water volumes and swimming.

Winter's example is a 10 × 12 m ice floor at -0.16 m, using `winter:ice_01`.
It has an opaque, non-solid depth backing at -0.31 m so the blended surface
does not expose the void. Existing snow drifts sit on dry shoreline supports;
the west and south approaches remain clear. Geometry is still prepared by the
offline compiler; only material traction descriptors are installed from its
validated source/materials at world load, preserving the collision binary format.

### The walkable step rule

**A rise of more than 0.4 m is refused; a drop of any size is walked off and
becomes a real fall** — ledges, pool decks and floor holes all lose support, so
treat every rill and drop-off as one the player can fall into. Staircases are
chains of floor regions whose consecutive offsets differ by ≤ 0.4 m (Places Demo
stair: 1.5 → 1.2 → 0.9 → 0.6 → 0.3 risers), and each rise is climbed instantly.
The rim's blocking face sits on the region boundary; its finite collider backing
extends 0.01 m under the higher floor. The radius-bounded horizontal sweep stops
tunnelling without filling narrow ramp lanes with hidden side boxes. The player
is supported by the walkable floor or a solid prop's top. Outside every authored
room there is no synthetic world floor; walking off the last surface starts a
real fall. Only empty-floor diagnostic worlds retain the legacy Y=0 plane.

While grounded, a rim **only blocks a change the player could not otherwise take**:
it carries the walkable step as headroom, so a player already within 0.4 m of the rim's top
(on a ramp or a staircase arriving beside the platform) walks past it. Its height
is sampled in short segments along the boundary, so a slope beside a rim is read at
its real local height instead of the cell's average. A rim never walls a landing
off. Airborne movement uses the physical body band: a jump must clear the
rim's actual top before crossing it. A trailing disc already overlapping a
walkable rim may move away from it during a fall without a forward push.

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
* **Transitions** are either ≤ 0.4 m risers (walkable up and down) or solid
  rims that can be walked off from above and fallen from; the drop below a rim
  is not an invisible floor. There are no ramps or sloped regions outside
  `ramps[]`.
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

### Water volumes: wading, swimming and surfacing

`water[]` authors bodies of water as a rectangular footprint
(`shape: "rect"`, the default) or a **circular** pool (`shape: "circle"`).
The surface draws as a translucent quad for a rectangle and a closed fan of
the same radius for a circle, and the player controller samples the same
shape, so what is drawn is exactly what is swum in — a circle has no invisible
square swimming area. A circle's rim is its wall line and is **dry**
(membership is strictly inside the radius), so its bounding box's axis
extremes are not water either. A water volume owns **no geometry of its own** — the
basin floor, its walls and its steps still come from the room and its
`floor_regions`; the volume adds the waterline and the behaviour. A prop that
authors `float` (section 19) rides the same resolved surface, and the level
validator proves its whole swept footprint stays inside one volume (inside the
disc, for a circle).

```json
"water": [
  { "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0,
    "surface_y": -1.65, "bottom_y": -3.0,
    "material": "core:water_pool_01", "opacity": 0.62, "swimming": true }
]
```

A **circular** hot tub or plunge pool authors its radius instead; `x`/`z`
stay the minimum corner of the footprint's bounding box, so the disc is
inscribed in `x..x + 2 * radius` and its centre is
`(x + radius, z + radius)`:

```json
"water": [
  { "shape": "circle", "x": 8.0, "z": 10.0, "radius": 2.5,
    "surface_y": -0.4, "bottom_y": -1.9,
    "material": "core:water_pool_01", "opacity": 0.62, "swimming": true }
]
```

Places Demo's pool room uses the circular form for its hot tub:
`core:hot_tub` is a joined tiled shell whose 0.13 m bottom flange covers the
chords of a 30-strip inscribed floor recess, and the water is one disc of
radius 1.25 m centred on the tub (`x`/`z` = `2.35`/`7.45`, surface `-1.65`,
bottom `-3.0`). Membership, the walkable recess, the collision rim and the
drawn 48-segment surface are the same circle, so no invisible square swimming
area exists; the rim stands 0.06 m proud of the deck. Three gentle `steam`
emitters sit just above the water (`hot_tub_steam_1..3`, count 10, lifetime
2.6 s), independent of the sauna's steam switch and bounded below the rim's
own air.

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `shape` | string | no | `"rect"` | `"rect"` or `"circle"`. Every field below resolves against it. |
| `x`, `z` | number | **yes** | — | Minimum corner of the footprint's bounding box (normalised, like a region). |
| `width`, `depth` | number | rect: **yes**; circle: no | — | `> 0`. A circle derives its bounding box from `radius` and may omit both, or stamp its own diameter (`2 * radius`) verbatim; a contradictory value is a named error. |
| `radius` | number | circle: **yes**; rect: no | — | `> 0`. The disc's radius; rejected on a rectangle. |
| `surface_y` | number | **yes** | — | World Y of the free surface, like a fixture's `y` — not relative to the floor. |
| `bottom_y` | number | no | lowest walkable floor under the footprint, else `surface_y - 2.0` | World Y of the bottom, sampled inside the rectangle or the disc's own interior. Metadata and depth reporting only; physics always stands on the walkable floor. Must be finite and strictly below `surface_y`. |
| `material` | string | no | `core:water_pool_01` | Surface material. The default is `alpha_mode: "blend"`, so any replacement must be translucent or the water reads as a solid lid. |
| `opacity` | number | no | `0.62` | `0.0..=1.0`; base surface coverage. The material opacity remains 1 so one material serves every volume. |
| `attenuation_per_metre` | number | no | `0.0` | Finite 0–16. Transmission is `(1-opacity) * exp(-attenuation_per_metre * vertical_depth)`. Zero retains exact existing coverage. This stylized vertical-depth approximation changes visual coverage only, not swimming/collision; it is not screen-space thickness or refraction. |
| `swimming` | boolean | no | `true` | `false` keeps the surface decorative: the player walks or falls through it, and no swim state can trigger. |

Medium/Full water surfaces receive ordinary incident-HDR floor charts and their
shadows, sky and practical-light sheen. Low keeps its vertex-light fallback.
Circular fans use one upward triangle plus a folded degenerate quad triangle;
coverage is applied once. Water and ice reuse bounded probe reflections with
Fresnel/gloss controls. All transparent world, prop, movable and character ranges
share stable back-to-front centre sorting, straight alpha and no depth writes.
Intersecting triangles and large coalesced ranges still require authored
separation. No refraction, ray tracing or new reflection pass is introduced.

How it behaves:

* **The surface** is one quad per rectangle and one closed fan per circle at
  `surface_y`, drawn through the
  material's `blend` contract in the sorted translucent pass with depth writes
  off and two-sided, so the basin floor, its walls and the submerged ladder stay
  visible from above and the surface is seen from underwater as well. A
  circle's fan has 48 segments and every rim vertex sits exactly on the
  authored radius; its triangles carry the same material, opacity and
  two-sided blend contract as a rectangle's quad. Its UVs
  continue the world tile grid, its colour carries the baked light of its
  rim (and centre) and it stays out of the lightmap atlas: a volume belongs to
  the water, not to a room's chart. In a prepared (`medium`/`full`) build the
  *bake* also treats the volume as transmissive with depth attenuation rather
  than as a lid, so the basin keeps the light a fixture above it can deliver
  (section 18).
* **Wading** is the ordinary walking controller: the water at the player's feet
  at or below `0.55 m` deep is waded at full walk speed, and a jump works
  normally. A standing player also keeps wading where the water is deeper than
  that as long as the floor underfoot is standable (within `EXIT_DEPTH` of the
  surface), so shallow basins read as chest-deep walking rather than floating.
* **Entering** is body based. The water under the feet must be a swimming
  volume deeper than `0.55 m` (`WADE_DEPTH`) and the walkable floor under the
  centre must *not* be standable (see Getting out): a wading player on a floor
  shallow enough to stand on never enters swimming at all. A falling player
  enters as soon as the water is deep enough — the plunge keeps its vertical
  velocity (bounded to twice `SWIM_RISE_SPEED`), so the fall decelerates in the
  water instead of grounding on the pool floor first; a body already sinking or
  resting enters once its eye is inside the surface band (`surface_y + 0.37 m`:
  the float margin plus the surface-swim margin); a **rising** player is never
  recaptured, so a ladder launch or a surfacing swimmer clears the waterline.
* **Swimming** moves at `0.55×` walk speed. The body sinks under the reduced
  underwater gravity (`SWIM_GRAVITY`, `-4.5 m/s²`) at a `1.1 m/s` terminal
  until the eye rests `0.55 m` above the floor (so a `1.35 m` basin — shallower
  than the standing eye height — still fully submerges), and holding Jump
  accelerates upward to `1.1 m/s` and holds the float line at
  `surface_y + 0.12 m` with a small idle bob, where the eye stays. Every
  branch is integrated in fixed `1/120 s` substeps, so entering, surfacing and
  releasing Jump move continuously, a released Jump's sink takes the same time
  at 30, 60 and 144 fps, and the eye is never written to the float line in one
  frame. Releasing Jump sinks again. A jump press never ground-jumps while
  submerged.
* **Getting out** is a bounded, cancellable climb, not a stand-up teleport. The
  walkable floor directly under the centre must be a standable exit at the
  waterline — at most one `WATER_EXIT_STEP_M` (`0.5 m`) above the surface or
  within `EXIT_DEPTH` below it (`1.23 m` standing, `0.43 m` crouched), with the
  stance's body clear overhead — and the eye must be near the top of the water
  (`surface_y − 0.6 m`). The climb raises the feet at `WATER_EXIT_CLIMB_SPEED`
  (`2.2 m/s`, the ladder speed) and derives the eye, so the camera rises
  continuously from the swim pose to the standing line (about 0.7 s from the
  deepest pool to the demo deck) instead of snapping up ~1.6 m; the final head
  clearance is validated once, when the climb begins. Horizontal movement keeps
  the ordinary swimming collision step throughout, so the player walks onto the
  real deck with ordinary input and real collision, and no horizontal position
  is ever written. A raised exit whose top sits above the camera is a
  **pull-up**: the disc keeps a body radius from the rim until the camera has
  cleared the rim top by the projection near plane (`SCENE_NEAR_M`, `0.1 m`,
  `WATER_EXIT_EYE_CLEARANCE_M`), so the rendered camera never clips into the
  deck it is climbing onto; the climb lifts the eye at
  `WATER_EXIT_CLIMB_SPEED` to that clearance line and holds it, the ordinary
  swim step then carries the centre over the few frames the hold lasts, and
  only then does the climb stand the body up. The pull-up needs the
  movement key held into the rim: releasing it cancels back to the swim pose,
  exactly like reversing. Reversing back over deeper water (the floor under the
  centre stops being a standable exit) cancels back to the swim pose at the
  current eye line: the surface pose over a floor deeper than `WADE_DEPTH`
  still counts as swimming, so a cancelled climb never falls through the air
  beside the rim. A higher standable floor that comes under the centre against
  the same waterline (the deck past a submerged walk-in step) is adopted as the
  climb's support, so the climb is never cancelled with the virtual feet
  embedded in it.
  Because the threshold is derived from the swim band, an exit can never
  immediately re-enter swimming and a pool edge cannot oscillate. The swimming
  wall band sits one `0.4 m` step below the surface so a ledge within a step of
  the waterline can be climbed, while a deeper rim stays solid, exactly like a
  floor-region rim on land; a solid prop or wall top is never a floor exit.
* **Overlapping volumes** resolve like overlapping floor regions: the last one
  authored at a point wins.
* **Validation**: non-finite values, a non-positive footprint or radius, a
  rectangle that authors a `radius`, a circle that omits `radius`, a circle
  whose `width`/`depth` contradict `2 * radius`, a surface at or
  below the floor beneath it, a `bottom_y` at or above the surface, a blank
  `material`, an `opacity` outside `0..=1`, a volume that overlaps no room, or
  more than 8000 volumes are named errors. A volume whose surface is below the
  floor it covers would be hidden inside the geometry, so it is rejected rather
  than silently invisible. An omitted `bottom_y` is resolved by sampling the
  volume's *own* footprint — a rectangle's centre and corners, or a circle's
  centre and four interior points — so a bounding box corner a square would
  wrongly include never decides a circle's depth.

Places Demo's pool is two adjacent volumes at one waterline: the basin
(`x 8..20, z 10..16`, surface `-1.65`, floor `-3.0` → 1.35 m deep) and the
submerged walk-in step (`x 10..16, z 16..16.9`, the same surface, floor
`-1.85` → 0.2 m of wading). Jump in from the deck, swim to the step and climb
out there, or climb the chrome ladder at the east rim. Walking off the deck is
a real drop into the water: the plunge decelerates, buoyancy surfaces you, and
climbing back onto the rim is a bounded 2.2 m/s climb, never a coordinate
snap.

### Ladders

`ladders[]` authors **climbable volumes**. The volume is the space the climber
moves through, not the rails: the visual rails stay a `core:pool_ladder` prop,
and the level authors the volume that matches them. A level with no `ladders`
array has no climbable geometry, exactly as before.

```json
"ladders": [
  { "x": 19.35, "z": 11.7, "width": 0.6, "depth": 0.6,
    "bottom_y": -3.0, "top_y": -1.5, "facing_degrees": 90.0 }
]
```

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Minimum corner of the climb volume's footprint (normalised, like a region). |
| `width`, `depth` | number | **yes** | — | `> 0`. |
| `bottom_y` | number | **yes** | — | World Y of the lowest climbable point, usually the floor at the base. Must be finite. |
| `top_y` | number | **yes** | — | World Y the feet reach at the top, usually the exit surface's own height. Must be finite and strictly above `bottom_y`. |
| `facing_degrees` | number | no | `0.0` | The yaw the climber faces while climbing, in the same convention as `spawn.yaw_degrees`: `0` climbs towards `-Z`, `90` towards `+X`, `180` towards `+Z`, `270` towards `-X`. Must be finite. |

How it behaves:

* **Attachment needs movement intent.** The player's cylinder must overlap the
  footprint, be behind the ladder's centre along `facing` (the approach side),
  and be pressing movement whose direction points along `facing`. There is no
  climb key: walking into the climbable face is the input. Contact from the exit
  side never attaches, so a player on the deck beyond the ladder is not pulled
  upward.
* **Climbing is collision-checked.** Holding the toward input rises at `2.2 m/s`
  and sideways movement keeps the ordinary wall/rim collision, so a rim between
  the climb volume and the deck stays solid until the feet reach `top_y`.
  Releasing the key holds position; backing away or pressing Jump detaches
  (Jump launches with the ordinary jump velocity); an overhead clamps the rise
  and the player stays attached below it.
* **The top lands on a real floor.** When the feet reach `top_y` and the
  walkable floor within a step is a real surface (the deck), the player steps
  onto it as a normal grounded transition. Author `top_y` at the exit surface's
  height; a `top_y` below a step edge can still be exited by jumping.
* **The prop must not be a wall.** A ladder prop authored `"solid": true` gives
  the whole rails bounding box a generic collision box, which blocks both the
  water approach and the deck exit. Author the prop `"solid": false` and let
  the `ladders[]` volume own the climb; Places Demo does.
* **Validation**: non-finite values, a non-positive footprint, a top at or below
  the bottom, a volume that overlaps no room, or more than 256 ladders are
  named errors.

Places Demo's ladder hugs the prop's west face at the east rim of the basin
(`x 19.35..19.95, z 11.7..12.3`, basin floor `-3.0` to deck `-1.5`, climbing
towards `+X`). A swimmer approaches from the water, holds forward, and tops out
on the deck at `-1.5`.

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
* **Their sides are real skirts.** A ramp's two sides are split at neighboring surface boundaries and drop
  only to the actual adjacent floor, ramp or stair tread; equal adjoining slopes
  emit no buried side. A staircase's sides close each step down to
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
| `baseboards[]` | `length`, `height` (default 0.09), `thickness` (default 0.018) | `material` (default: the level's wall) | **No collision.** The back face is not drawn (it is buried in the wall), and the run stands proud of the wall plane, so it never shares a plane with it. The loader rejects a board whose whole cross-section is inside a wall solid. A wall material may also declare an automatic trim (see *Automatic trim* below). |

**Automatic trim from the catalog.** A wall material may declare a `baseboard`
material id in `assets/catalog.json` (section 14). At load, every wall length
face whose resolved material declares one — the per-face `faces` override, else
the wall's `material`, else `defaults.wall` — receives baseboard runs in that
trim material, appended **after** any authored `baseboards`. A face qualifies
only when it fronts one walkable floor along its whole length (sampled just
outside the face at both inset ends, the middle and the quarter points; every
sample must exist and agree within 0.05 m) and the wall's own base meets that
floor within 0.05 m — a floating or half wall, or a wall along a step, is
skipped. Each generated run is pinned to the floor it fronts, uses the default
height and thickness, and is split around every opening whose sill reaches the
board: a floor-level door or passage is never covered, while a window with a
sill keeps its board. A run an authored `baseboards[]` entry already covers on
the same plane is not generated, so hand-placed trim always wins. Author
`baseboards` explicitly when you want a different material or height, or a run
the automatic pass would not place.

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

### Arc walls and circular pillars

Two data-authored primitives cover the curved vocabulary without any code
change: an **arc wall** is a curved slab on a circular plan, a **circular
pillar** a solid round post. Both are placed by the **centre of their circle**
(not a corner), take any catalog material id, and are tessellated into flat
segments the way the archway is.

```json
"arc_walls": [
  { "x": 14.0, "z": 4.0, "radius": 1.0, "thickness": 0.24,
    "height": 2.2, "start_degrees": 180.0, "sweep_degrees": 180.0,
    "segments": 16,
    "material": "core:wallpaper_stained_01",
    "inner_material": "core:pool_tile_wall_01",
    "cap_material": "core:baseboard_office_01",
    "end_material": "core:baseboard_office_01" }
],
"pillars": [
  { "x": 3.5, "z": 8.5, "radius": 0.4, "height": 3.0, "segments": 24,
    "material": "core:pool_tile_wall_01",
    "cap_material": "core:baseboard_office_01" }
]
```

| Field (arc wall) | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Centre of the arc's circle, world coordinates. |
| `radius` | number | **yes** | — | Centreline radius, `> 0`. |
| `thickness` | number | no | `0.3` | Ring thickness across the radius; `> 0` and `< 2 × radius` (`inner_radius` must stay positive). |
| `y` | number | no | walkable floor under the arc's mid-span centreline | Absolute world Y of the base. |
| `height` | number | no | local clear ceiling per segment | Height above the base; authored is rigid, exactly like a wall's. |
| `start_degrees` | number | no | `0.0` | Compass angle of the first end (0 = north, +90 = east). |
| `sweep_degrees` | number | no | `90.0` | Signed sweep; positive runs north → east → south → west. Non-zero, `≤ 360`. |
| `segments` | integer | no | 24 per full circle, scaled to the sweep | Tessellation across the whole sweep, 3–128. |
| `material` | string | no | `defaults.wall` | Face material for both length faces, caps and ends. |
| `inner_material`, `outer_material`, `cap_material`, `end_material` | string | no | `material` | Per-face overrides (concave face, convex face, top/bottom caps, the two radial ends). |
| `shine` (+ the four face `_shine` keys) | number | no | material default | Per-surface glossiness `0.0`–`1.0`. |

| Field (pillar) | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | Centre, world coordinates. |
| `radius` | number | **yes** | — | Solid radius, `> 0`; the pillar is solid to its axis. |
| `y` | number | no | walkable floor under the centre | Absolute world Y of the base. |
| `height` | number | no | local clear ceiling | Height above the base. |
| `segments` | integer | no | `24` | Tessellation of the full circle, 3–128. |
| `material`, `cap_material` | string | no | `defaults.wall` / `material` | Body and top cap. |
| `shine`, `cap_shine` | number | no | material default | Per-surface glossiness. |

Rules that matter:

* **Angles are compass yaw.** `start_degrees` is the direction from the centre
  to the arc's first end; increasing angles sweep clockwise seen from above
  (north → east → south → west), matching prop and guardrail yaw. A
  `sweep_degrees` of exactly `360` (or `-360`) is a full ring: it closes on
  itself and emits no end caps.
* **Degenerate dimensions are rejected by name**: a non-positive radius, a
  thickness at or above twice the radius, a zero or over-full sweep, a
  `segments` outside 3–128, a non-positive authored height and blank material
  ids each produce a `Arc wall {i} …` / `Pillar {i} …` error at load.
* **Texture coordinates tile at the material's world scale.** `u` is the arc
  length along each face's own circumference (the concave face uses its own
  smaller radius, so nothing is stretched around the sweep) and `v` is height;
  caps use the ordinary world plan mapping. Assign any catalog material —
  including tiled, glossy or normal-mapped ones — and it behaves exactly as it
  does on a flat wall.
* **Collision is derived from the same geometry.** An arc wall contributes a
  short run of AABBs per rendered segment (its ring band), and a pillar
  contributes horizontal rows of its rendered polygon; neither ever falls back
  to one rectangle around the whole circle. A curved wall is exactly as
  passable as it looks, and a pillar's collision follows its silhouette. The
  baked lighting occluders use the same solids, so curves cast real baked
  shadows.
* **The tops are landable** where they are exposed: a low pillar or arc wall
  supports the player, and a pillar whose top meets the ceiling skips its cap
  so it never fights the ceiling plane.
* **No decals on curved faces.** Decals are planar quads; a decal placed over a
  curved wall would float or clip. Use a curved primitive's own per-face
  materials instead, or place a decal on a flat wall/ceiling/floor nearby.
* **Tessellation is the visual and collision budget.** The default 24-segment
  full circle reads smoothly at room scale; the checker reports a `curve-coarse`
  warning when a primitive's sagitta exceeds 2 cm, and `segments` up to 128 are
  accepted.

Places Demo's communal shower bay is the shipped use of both primitives, and
the pattern to copy: a tiled wet room east of the pool hall (`x 26..33`,
`z 7..11`, floor `-0.9`, reached from the deck by two 0.3 m `floor_regions`
steps through a 1.2 m passage), a single 90-degree `arc_walls` entry
(centre `29.4, 6.6`, radius 2.4, thickness 0.22, height 2.1, `segments 12`,
pool wall tile with a brushed-metal cap) whose concave face shelters three
shower positions, and three thin full-circle `pillars` (radius 0.05) as the
chrome risers with `thresholds` arms and disc heads above them. A
`floor_patches` band of the deck tile at a raised `shine` carries the wet
sheen (the demo keeps the pool's single planar mirror), three small
brushed-metal patches read as drains, a tiled `half_walls` bench sits against
the sauna wall, and four `core:pool_light_round` fixtures light the bay. Every
part reuses an existing material and none of it is a new asset: the curve is
data, its collision comes from the same geometry, and the bay connects to the
sauna through a second `sauna` door whose floor is flush on both sides.

---

## 11. Materials

Colour PNGs are authored sRGB; material tint/base factors, light colour and
intensity are linear values. Bake and runtime shading retain linear HDR, and
presentation alone applies exposure and display encoding. Normal maps and
emission masks are numeric data. Changing Lightmaps does not change texture
size, mips or the independently selected Texture Filtering policy. See
[Asset specification §§16–18](ASSET_SPECIFICATION.md#16-formats-and-colour) and
[Stage 2 contracts](art-style/stage2/contracts.md). No map-specific brightness
compensation is needed or supported by this contract.


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
| `tile_metres` | number | `2.0` | `0.05`–`64`. World metres covered by one repeat, both directions. Only a material may declare it. | Uses `2.0`, the standard sheet size. |
| `tint` | `[r,g,b]` | `[1,1,1]` | Each channel `0.0`–`1.0`. Static multiply on the sampled texture. Only a material may declare it. | White; no tint. |
| `surface` | string | none | `wall`, `floor` or `ceiling`. Documentation/validation only; geometry decides which family a material draws on, so any material may legally be used on any surface. | No surface tag. |
| `ground_surface` | string | `"normal"` | `"normal"` or `"ice"`; material definitions only | Physical traction on supporting floor surfaces; unrelated to visual gloss. |
| `emissive` | `[r,g,b]` | none | Each channel `0.0`–`1.0`. Adds an emissive term on top of baked light. Only a `material` `definition` may declare emission. | The surface does not emit. |
| `emissive_intensity` | number | `1.0` when `emissive` is set | `0.0`–`8.0`. Multiplier on the emissive colour. Asserting it without `emissive` is a catalog error. | `1.0`. |
| `emissive_mask` | string | none | Logical id of a file-backed `texture`. Its RGB modulates where the surface emits; it must resolve or the whole material degrades to the diagnostic texture. Asserting it without `emissive` is a catalog error. | No mask; the material's own texture modulates the glow. |
| `normal_texture` | string | none | Logical id of a file-backed `texture` holding a tangent-space normal map (RGB = x/y/z encoded `0..255 → -1..1`). Must resolve or the whole material degrades. | No normal perturbation. |
| `normal_strength` | number | `1.0` | `0.0`–`2.0`. Multiplies the decoded map's `xy`. Asserting it without `normal_texture` is a catalog error. | `1.0`. |
| `specular` | number | `0.0` | `0.0`–`1.0`. Sheen strength: how much light the surface catches. `0.0` is the default flat look, and no `shine` value can switch a sheen on. | No sheen. |
| `specular_color` | `[r,g,b]` | white | Each channel `0.0`–`1.0`. Sheen colour; it does **not** require `specular`. With `specular: 0` the whole sheen term is zero, so the colour has no visible effect. | White sheen. |
| `shine` | number | `0.4` | `0.0`–`1.0`. Glossiness: `0.0` matte, `0.25` slight sheen, `0.5` semi-gloss, `0.75` polished, `1.0` extremely glossy. Shapes both the sheen and the reflection; a material with `specular: 0` never sheens at any shine. **Not a mirror** — a mirror is `reflection_mode: planar`. | `0.4`. |
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
* `doors[].material` / `.frame_material` / `.handle_material` (each kind has its
  own defaults; see [§30](#30-doors-switches-and-effects))
* `effects[].material` (the steam billboard sheet)

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

The renderer multiplies: **decoded linear texture × linear material tint × linear incident light**, where
*baked light* is the lightmap atlas texel for static world geometry (see
[Lighting](#18-lighting)) and the baked vertex colour on the vertex-lit fallback
path. Colour PNGs decode sRGB before filtering; normal maps, emission masks and
alpha remain numeric. Presentation encodes sRGB once after HDR composition.
Author with the linear tint in mind (the office wallpaper tint, for example,
is `[0.85, 0.80, 0.42]`, so the PNG is authored pale).

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

**Quality levels.** `Medium` and `High` draw the response; `Low` leaves the
normal-map and sheen terms out and keeps albedo × light × emission × alpha.
Every level uses the same materials and the same PNGs — the difference is one
shader gate, not a second art set.

**Art direction.** A normal map on this renderer is *detail on a flat surface*,
not a substitute for geometry: it cannot cast a shadow, it does not change the
silhouette and it is lit only by the baked room light. Keep the low-poly
vocabulary — bumps, grime, brushed streaks and panel seams, not sculpted detail.

### Selective reflections: which surfaces reflect

Two authorable paths can show a surface the room back: a **static probe** (a small
cubemap captured offline and installed from the package) and a **planar mirror** (a real second view of the
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
| `probe` | the packaged static HDR cubemap, read by the reflected view vector | one texture read per reflective fragment |
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
* **It is approximate.** A probe is a 64-texel-per-face cubemap at Reflections
  `Full` (48 at `Medium`, absent at `Off`) — the shape of the room, not a second render of it —
  and a planar reflection is drawn at half resolution. Use them where the surface
  should read as wet, polished or mirrored, not where the player will compare the
  reflection with the room.
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

**Independent reflection quality.** Reflections `Full`/`Medium` use 64/48-texel
probes and allow the half-resolution planar pass; `Off` retires probes and the
planar target. The overall High/Medium/Low presets select Full/Medium/Off,
respectively, but the player may override Reflections independently. Low with
Reflections Full is supported. An Off reflection keeps the material's sheen.

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

* World batches, static props, movable objects and characters share stable
  back-to-front centre sorting in both scene and emission passes. Sorting is
  per batch/object, not per intersecting triangle; large overlapping transparent
  ranges and several primitives inside one object still need careful placement.
* A translucent surface never occludes anything: a pane of glass is hidden by the
  wall it sits in, but does not hide the room behind it in the depth buffer.
* Emissive translucent materials work: emission is added to the lit term before
  the alpha blend, so a backlit sign is *both* bright and see-through. Author it
  with `emissive` plus `alpha_mode: blend` (Places Demo's
  `core:glass_sign_lit_01` is exactly that).
* Decals are unaffected: they are their own pass with a cut-out and a depth bias,
  authored as decal sheets (section 17), not as materials.
* A `blend` material with `opacity: 0` is invisible and is skipped entirely.
* **Rendering transparency and physical collision are independent.** A material's
  `alpha_mode` decides only how the surface is drawn; whether a glazed opening
  stops the player is the opening's own `solid` flag. `"solid": true` on an
  opening with `glass` adds the pane's thin collision slab, while a
  `solid: false` pane is a pure visual surface — each combination (opaque solid
  panel, walk-through glass, solid glass, cut-out walk-through grille) is
  legal and explicit.
* Transparency uses straight alpha without refraction. Prepared transport
  attenuates blended crossings by `1 - coverage`, and cutout holes transmit
  while covered texels block. A pane is distinct from an empty aperture.
* GLB MASK props use the cutout pass on every route and transmit prepared rays
  through their holes. BLEND uses the translucent pass on static, character and
  dynamic routes; its prepared straight transmission follows the same coverage.

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
  seen from both sides. Medium/Full panes receive incident-HDR lightmap charts;
  Lightmaps Off keeps the supported vertex-lit fallback.
* Collision follows the wall's solid slices (a raised window still blocks) plus
  the opening's own `solid` flag: a glazed opening with `"solid": true` adds the
  pane's thin slab, so glass can be a real barrier; the default `false` keeps a
  pane walk-through. Collision and lighting coverage are independent: opaque
  panes block prepared rays, cutout holes transmit, and blended panes attenuate
  by real alpha coverage. Tint affects drawn colour, without coloured refraction
  or absorption in the pane's transmitted light.
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

### Authored presentation and global atmosphere

Optional `environment` controls the final image without changing stored light:

```json
"environment": {
  "presentation": {"exposure": 1.0, "tone_knee": 0.75,
                   "saturation": 1.03, "contrast": 1.02},
  "fog": {"color": [0.60, 0.63, 0.68], "density": 0.0095,
          "reference_y": 2.0, "height_gain": 0.045}
}
```

Omission or an empty object retains these defaults. Unknown nested controls are
rejected. Exposure is a fixed linear multiplier (0.125–8), never automatic;
`tone_knee` (0.25–0.95) starts a smooth rational highlight shoulder. One shared
RGB scale preserves highlight hue. Saturation and contrast (0.8–1.2) are
restrained display-space adjustments after one sRGB conversion. Every quality
preset shares these controls for static surfaces and entities; Low/Medium now
retain the High default presentation. Quality still changes scene resolution,
lighting and resource budgets. HUD composition remains outside the scene curve.

Bloom is the independent player preference. Only material/fixture emission
enters its linear HDR source. A soft threshold starts at 0.5 HDR and strength
0.22 limits the halo: brightly lit walls never bloom. Coverage, fog and storm
transmission also attenuate emission; transparent nonemitters attenuate bloom
behind them. No exposure or tone curve is stored in lightmaps, probes or
reflection targets. Development diagnostic modes bypass exposure, tone, grade,
bloom, world fog, sky, decals and effects; their explicitly mapped linear values
receive one sRGB encoding. `final` uses the normal image path.

### Fog: authored global atmosphere plus level-authored regions

`environment.fog` supplies the existing squared-exponential distance haze and
bounded height gain before presentation. `color` is display sRGB (three 0–1
channels), decoded once before mixing with HDR light. `density` is finite
0–0.5 per metre; `reference_y` is finite world metres (−10000–10000), and
`height_gain` is finite 0–1 per metre below the reference. Defaults retain the
established global atmosphere. This haze is separate from weather extinction;
severe snow keeps its own shelter-aware sightline transmission and fog colour.
The infinite sky retains its background treatment and existing storm blend.

On top of it a level may author `fog_regions`: world-space boxes that thicken the
air **inside** them, per fragment. They are the supported way to build mist over a
yard, a pool of cold air in a sunken room, or a low morning layer without a
particle pass.

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `id` | string | **yes** | — | Non-empty, ≤ 64 characters, unique in the level. |
| `min` | `[x, y, z]` | **yes** | — | World-space minimum corner, finite; strictly below `max` on every axis. |
| `max` | `[x, y, z]` | **yes** | — | World-space maximum corner, finite; strictly above `min` on every axis. |
| `density` | number | **yes** | — | Extinction per metre inside the layer, `0.0..=0.5` (`MAX_FOG_REGION_DENSITY`). |
| `color` | `[r, g, b]` | no | the global fog colour | Each channel `0.0..=1.0`. |
| `falloff_m` | number | no | `2.0` | Horizontal soft edge **inside** the box, in metres, `>= 0`; `0.0` is a hard edge. |
| `ground_y` | number | no | `min.y` | World Y where the full-density ground layer starts. |
| `top_y` | number | no | `max.y` | World Y the density has faded to zero at. |

Per fragment, with `p` the fragment's own world position:

* `horizontal_edge_factor = clamp(min(p.x - min.x, max.x - p.x, p.z - min.z, max.z - p.z) / falloff_m, 0, 1)` — zero outside the box, a linear ramp to full over the last `falloff_m` metres before each side face.
* `vertical_factor = 1` at and below `ground_y`, `clamp((top_y - y) / (top_y - ground_y), 0, 1)` between them, `0` above `top_y`. A layer with `top_y <= ground_y` is a half-space: full below its base, nothing above.
* The region's effective contribution is `density * horizontal_edge_factor * vertical_factor`.
* **Regions never sum.** The region with the greatest effective contribution wins; a tie keeps the lowest authoring index. The global atmosphere's own density term is **added** to the winner's, and the colour mixed towards is the winner's colour (the global colour when no region contributes).

Two concrete examples:

```jsonc
"fog_regions": [
  // A thin low mist lying on the yard: half a metre deep, a long 8 m soft edge,
  // light density. Its ground layer starts at the yard ground.
  { "id": "yard_mist", "min": [-2.0, -0.1, -92.0], "max": [26.0, 0.5, 0.5],
    "density": 0.06, "color": [0.6, 0.63, 0.68], "falloff_m": 8.0,
    "ground_y": -0.1, "top_y": 0.5 },
  // A tall, denser layer stacked over the same yard. Where both reach, the
  // denser layer wins outright and the thin mist does not add to it.
  { "id": "yard_depth", "min": [-2.0, 0.0, -92.0], "max": [26.0, 6.0, 0.5],
    "density": 0.12, "color": [0.35, 0.4, 0.5], "falloff_m": 6.0,
    "ground_y": 0.0, "top_y": 5.0 }
]
```

An **indoor room adjacent to a fogged exterior stays clear**: the hall inside the
building lies outside both boxes, so a fragment on its walls evaluates only the
global atmosphere — even while the camera stands in the yard fog looking in through
an open door. The test is the *fragment's* world position, never the camera's; a
fragment inside a region is fogged regionally even when the camera is far away.

Cost, presets and recovery:

* `fog_regions` is capped at **16** (`MAX_FOG_REGIONS`); the loader rejects a 17th by name.
* The fragment shader loops over a fixed 16-entry uniform array bounded by the live count, so there is no per-frame CPU work and no per-frame rebuild. Adding a region costs one uniform write at level install.
* **Quality presets upload a prefix of the authored list**: `Low` the first `min(count, 2)` regions, `Medium` the first `min(count, 8)`, `High` all 16. The count is a uniform value, so switching presets recovers the dropped regions instantly with no level rebuild and no geometry change. Author the layers you care about most first.

Interaction notes:

* Fog is applied once per fragment, after lighting/emission/reflection, so floors, walls, void walls and translucent panes all mix with the same current fog. A translucent surface blends with the fogged image behind it like any other translucent draw.
* The **sky is not fogged**: it is an infinite background, so a regional layer thins the world towards its colour while the stars stay crisp. Pick a colour that sits with the sky sheet.
* Water surfaces take the same single fog term as any other world surface; there is no second underwater fog.
* `fog_density == 0` and no regions is the historical raw pass-through: a level that authors no regions is pixel-identical to before this feature existed.

### Void wall and floor boxes

`void_walls` places real opaque box surfaces that hide the void a level did not
build: the outside of an exterior shell, the underside of a raised platform, the
back of a one-sided room. They are ordinary material surfaces drawn through the same
static-mesh pipeline — never a fade, a black plane or an overlay — so they block
sight because they are geometry, take the global and regional fog exactly once, and
shade with the catalog material named on them.

| Field | Type | Required | Default | Constraints / semantics |
| --- | --- | --- | --- | --- |
| `id` | string | no | — | Optional; non-empty, ≤ 64 characters, unique when present. |
| `min` | `[x, y, z]` | **yes** | — | World-space minimum corner, finite; strictly below `max` on every axis. |
| `max` | `[x, y, z]` | **yes** | — | World-space maximum corner, finite; strictly above `min` on every axis. |
| `material` | string | **yes** | — | An existing catalog material id; every emitted face draws it. |
| `faces` | enum | no | `"inward"` | `inward` (the box seen from inside), `outward` (from outside) or `both`. |
| `solid` | bool | no | `true` | The authored box collides, exactly like a solid prop's `size` box. |
| `occludes` | bool | no | `true` | The authored box joins the baked-light occluder set like a solid prop. |

`faces` selects which of the six box faces are emitted and which way their normals
point: `inward` emits the six faces wound towards the box interior (a shell around
the space the camera stands in), `outward` the six wound away from it (a slab or
plate the camera looks at), `both` two quads per face. The engine shades both sides
(it never enables backface culling), so an emitted face blocks sight from either
side and reads as the material on each; `faces` decides the authored normal each
side carries, not whether the surface exists.

* **Collision follows the whole box**, exactly like a solid prop's `size` box: `solid: true` adds the authored volume to collision and `solid: false` adds nothing. A thin slab (a boundary wall, a floor plate) behaves exactly as expected; a box that *encloses* a walkable space is a solid block, so build a hollow enclosure from thin slabs or set `solid: false`.
* **Bake participation is exactly a solid prop's.** In the prepared (Medium/High lightmap) transport solve, the box's drawn faces are ordinary opaque mesh triangles, so they block and bounce light exactly like a prop model's; the `occludes` flag additionally contributes the whole box to the fast occluder set the baseline/visibility bake and the vertex-lit path test (`LevelLighting::bake`). `occludes: false` therefore removes the box from the box-occluder set only — the prepared solve still sees the drawn faces, exactly as it sees a prop whose `occludes` is false. Use a transparent material if a surface must transmit.
* An `occludes` box blocks baked light exactly like a solid prop's occluder box: it joins the bake's segment-test occluder list and shades its surroundings, but it deliberately does **not** answer the bake's wall-only room-partition queries, so a horizontal floor plate never buries the floor samples above it. A vertical slab is not a room partition for the bake; pair it with real `walls` when the bake must split a space.
* **A level keeps its sky unless a box covers it.** A shell only affects what the author encloses; an opening with no box in front of it shows the sky exactly as before.
* A void wall is not a light and not a fake dark overlay: it never emits. Give it a real material, and choose a dark material if it should read dark.

Two concrete examples:

```jsonc
"void_walls": [
  // The outdoor yard shell: one enclosing box seen from inside. It is a visual
  // shell only - the level's own walls and pegs own movement and the fast
  // occluder set - so it does not collide and does not darken the baseline.
  { "id": "yard_shell", "min": [-2.0, -0.2, -92.0], "max": [26.0, 9.0, 0.6],
    "material": "outdoor:dirt_gravel_01", "faces": "inward",
    "solid": false, "occludes": false },
  // The underside of a raised platform: a thin floor plate whose visible face
  // is its bottom (an outward-facing box), solid and occluding like a prop.
  { "id": "platform_underside", "min": [4.0, 2.8, 4.0], "max": [8.0, 3.0, 8.0],
    "material": "outdoor:concrete_pavement_01", "faces": "outward" }
]
```

Limits: `void_walls` is capped at **256** entries; the loader rejects a 257th by
name. Each box emits six quads (twelve for `"both"`), so the geometry estimate
budgets `MAX_VOID_WALL_QUADS` per entry.

### Community packages

Level packs are not supported; a level is compiled to a `.placesmap` package.
The package carries the prepared world and its dependency identities, and the
player never reads authoring sources or pack textures. Pack material
definitions (`pack:` ids) have no producer in the current workflow.

To ship custom artwork with a map, register it in `assets/catalog.json` like any
other texture or prop. Self-contained community packages with embedded texture
payloads are part of the package format
(`kind: "embedded"` dependencies), but the current compiler builds maps against
the installed asset bundle; see [PACKAGE_FORMAT.md](PACKAGE_FORMAT.md).

## 12. Textures

**Repository rule: normal editable game textures must exist as real image files in
the asset tree.** Do not generate normal game artwork procedurally from Rust/source
code at runtime. Create the PNG, register it in the catalog, reference it by logical
id. The fixed internal diagnostic/UI sheets are also PNGs, with their original
layouts preserved; see the table below and Asset Specification §12.3.

### Supported formats and limits

| Property | Value |
| --- | --- |
| File format | **PNG only.** Signature-checked; RGB, RGBA, grayscale, grayscale+alpha and palette (with/without `tRNS`) all normalise to 8-bit RGBA; 16-bit is stripped to 8-bit. |
| Hard edge limit | **1024 px** on either edge, enforced by the runtime decoder for ordinary PNGs (surfaces, decals, fixtures, embedded GLB images); sky panoramas have a separate 2048×1024 ceiling: `texture dimensions {w}x{h} exceed the 1024x1024 limit`. |
| Preferred edge | **256 px** — a soft tooling/policy warning, not a runtime error. |
| Surface sheets | **Square** (both edges equal) per shipped-asset policy; the runtime accepts another shape, but a `tile_metres` cell would stretch. |
| Decal sheets | **Power-of-two on both edges** (mipmapped fitted sampling; POT is the shipped-asset policy). |
| Fixture faces | **Power-of-two on both edges** likewise. |
| Decoded surface budget | ≤ 4 MiB RGBA8 per sheet (one 1024×1024 sheet is exactly 4 MiB). |
| Surface wrapping | `REPEAT` + mipmaps. Surfaces are expected to tile. |
| Decal wrapping | `REPEAT` + mipmaps, but full-sheet fitted UVs, so the sheet never actually repeats. |
| Fixture wrapping | `CLAMP_TO_EDGE` + mipmaps; the whole sheet is fitted once across the face. |
| Alpha | Surface sheets are opaque *unless* their material authors `alpha_mode`. The base pass has blending off, so an `opaque` material's alpha channel is ignored; `cutout` discards texels below the material's cutoff and `blend` samples it. Decals are alpha cut-outs (alpha < 0.5 discarded). Fixture faces are opaque. |
| Normal maps | A normal map is an ordinary RGB sheet in the same asset tree; the material names it with `normal_texture`. It is a *surface* texture for quality purposes (High 1024 / Medium 512 / Low 256), tiles like its albedo and may be hand-painted or generated. |
| Colour space | Colour PNG RGB decodes sRGB before filtering; tint/light factors and lighting use linear HDR. Numeric normal/mask/alpha data do not decode sRGB. Presentation encodes once. |

Shipped Office/Pool surface sheets are intentionally 1024×1024, square, opaque;
`python3 tools/textures/build.py --check` reports them as "over preferred" warnings by
design. The preferred 256 px size is a budget warning, not a rejection.

### Runtime quality levels and downscaling

The source PNG is *not* what necessarily reaches the GPU. Three quality levels
(`settings.json` → `"quality": "low" | "medium" | "high"`, default `high`)
decide a **runtime** edge budget
per texture class. The level is a selector in Settings → Graphics and can be
changed while a level is running: staged loading installs the corresponding
prepared package variant and GPU resources, preserving player, camera and game
state. The player never bakes a new atlas or captures a probe:

| Texture class | High (default) | Medium | Low |
| --- | --- | --- | --- |
| Surface sheet | 1024 | 512 | 256 |
| Fixture face | 1024 | 512 | 256 |
| Decal sheet | 1024 | 512 | 256 |
| Prop sheet (GLB) | 256 | 256 | 128 |
| Emissive mask | 512 | 256 | 128 |
| Sky panorama | 2048 | 1024 | 512 |
| Preset Lightmaps | Full: 1024, 16 texels/m | Medium: 1024, 12 texels/m | Off: vertex lighting |
| Preset Reflections | Full: 64-pixel probes + planar | Medium: 48-pixel probes + planar | Off |
| Prepared transport | Full solve | Medium solve | Historical vertex fallback |
| Legacy coarse prop-occlusion grid | 0.075 m | 0.11 m | 0.15 m |

* **High is the native Places runtime size.** Every shipped asset is already at
  or below it, so High uploads the decoded image unchanged — no rescaling, no
  visual change. A native 256×256 prop sheet stays 256×256.
* **Medium and Low use the same assets** and box-filter each image once, at
  level load, to the level's budget. A lower level is not a second art library;
  ids, materials and geometry are identical. It is an optional
  quality/performance trade, never a repository asset requirement.
* Downscaling happens once per upload, never per frame, and the result is cached
  with the texture it produced. Every level is deterministic: the same source
  always produces the same runtime image.
* The source hard limit (1024 px for ordinary sheets, 2048×1024 for skies) is unchanged by any level: quality only
  decides how much of an accepted source reaches the GPU.
* Lightmaps and Reflections are independent advanced controls. The compiler
  prepares Off/Medium/Full variants of the same source; the player selects a
  packaged variant, without a separate authored level or artwork set. The
  legacy two-profile planner API also supports 512-pixel/10-texel Low atlases,
  but that API budget is not the runtime Low preset, which selects Off.
* Do **not** author a separate level or asset set for a lower quality level; one
  level serves all three.

An author does not need to do anything differently for Medium or Low: ship the
sane source size and let the engine fit it.

**Texture Filtering** is a separate player setting (Settings → Graphics),
independent of the quality level: Low, Medium and High all use trilinear
filtering with a full mip chain and request approximately 4×, 8× and 16×
anisotropic filtering respectively. It never changes which PNG a map uses, so a
map author needs no per-asset filtering choice (see
[ASSET_SPECIFICATION.md](ASSET_SPECIFICATION.md) §15).

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

### Internal image resources (not the authoring path)

| Resource | Where | Purpose |
| --- | --- | --- |
| Missing-texture diagnostic (64×64 magenta/black) | `assets/core/textures/missing_01.png` (embedded by the engine) | Visible fallback for any broken surface/decal/fixture texture. |
| Validation decal atlas (256×256; only `core:decal_test_01`) | `assets/core/decals/validation_atlas_01.png` | Fixed atlas slots, alpha and bottom-up row convention; the existing atlas UV mapping is preserved. |
| White sheet (1024×1024 opaque white fill, file-backed) | `assets/core/textures/white_01.png` (loaded by `src/render/wgpu/texture.rs`) | Untextured geometry (fixture housings, UI quads). |
| HUD font atlas (128×64) | `assets/core/ui/font_01.png` | Project-owned bitmap UI font with sixteen 8×8 ASCII cells per row and the reserved white UI cell. |
| Emergency white sheet (2×2) | `assets/core/textures/white_fallback_01.png` | Embedded opaque white fallback if the primary embedded white PNG cannot decode. |
| Lightmap atlas (Full up to eleven pages per contribution group; lower profiles eight) | `src/lighting/lightmap/`, packaged KTX2 payloads | Incident linear HDR and signed moments computed offline from geometry/material/light inputs. Loaded from the prepared package; the 320 MiB +64 KiB typed bound and unchanged aggregate/shape guards apply. PNG dumps are diagnostics, not artwork. |

Everything else the renderer draws from an image comes from a PNG under `assets/`.
Every *surface, fixture, decal and prop texture* is still a real PNG asset under
`assets/`, including the lightmap's albedo partners. Prepared atlases and probe
cubes are derived lighting data in `.placesmap` archives; the realtime planar
target is rendered scene data. None is runtime-generated texture artwork.

### Texture budget summary

| Budget | Value | Enforced by |
| --- | --- | --- |
| PNG hard edge | 1024 ordinary; sky 2048×1024, 2:1 POT | runtime decoder + `tools/textures/build.py` + package tests |
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
| `home:tile_home_01` | floor | Kitchen / bathroom / utility tile | 30 cm tiles at a 1.2 m repeat |
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
`core:plant`) remain available. The Home concept reconstruction uses local
`home:sofa`, `home:armchair`, `home:coffee_table`, `home:dining_table`,
`home:dining_chair`, `home:tv_console`, `home:bookshelf`, `home:rug`,
`home:floor_lamp`, `home:sink` and `home:stove` variants to match its cream,
timber and enamel palette without changing shared core artwork. Static
`home:mug`, `home:book_stack`, `home:cushion`, `home:outlet`,
`home:cabinet_strip`, `home:landscape_frame`, `home:kettle`, `home:toaster`
and `home:door_casing`
fill depicted gaps. `home:backsplash_01` is a 15 cm ceramic wall material.
These use the ordinary prop/surface contracts; their fitted atlas layouts
and master/native sizes are recorded in ASSET_SPECIFICATION.md §8.9.

`tools/levels/refine_home.py` is the idempotent Home-only demo dressing
source. Prop y remains floor-relative, so a coffee mug on the new 0.46 m
table uses y=0.46 even though the room floor is at -0.9 m. Cabinet strips
mount at y=1.424 and own small warm line lights. They do not acquire switch
behaviour. Warm paint, oak and two ceiling variants use local `_warm_01`
materials, preserving the original shared images. The backsplash front is
at z=3.231; outlet backs sit at z=3.230, meeting the tile face within 1 mm.
The existing encounter entities, interactive switches and doors,
stair support and loft structure are preserved. The current connected Home
is a vaulted loft rather than the concept's outdoor residential balcony.

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
anywhere).

### Entry field reference

| Field | Type | Requirement | Default / fallback | Applies to |
| --- | --- | --- | --- | --- |
| `id` | string | **required** | — | all. Non-empty; characters `[A-Za-z0-9:_.-]`, may not start with `:`. Duplicates are a catalog error. |
| `display_name` | string | optional | the id | all |
| `asset_class` | string | **required** | — | all. Validated lower-case slug; shipped: `environment`, `entity`, `core`, `diagnostic`. Unknown classes parse in Rust but fail `validate.py`. |
| `theme` | string | optional | none (generic) | all. Organizational only; must be a declared theme to satisfy `validate.py`. |
| `asset_type` | string | **required** | — | all. Shipped: `prop`, `entity`, `material`, `texture`, `light`, `decal`. |
| `source` | `"file"` \| `"definition"` \| `"generated"` | optional | inferred: `texture` → `definition`; else `model` → `file`; else `generated` | all. `file` requires a `model`; `generated` must not declare one; `definition` requires a `texture` and must not declare a `model`. |
| `model` | string | type-specific | — | Resource path **relative to `assets/`**. `.glb` for props/entities; `.png` for file textures, file decals and file lights. Rejected if absolute, backslashed or containing `..`/empty components. |
| `size` | `[w,h,d]` | optional (validator requires it for placeables) | invalid values dropped; runtime fallback `[0.6, 0.9, 0.6]` | props, entities. Used by placeholder boxes; a level's own `size` overrides it for collision. |
| `color` | `"#rrggbb"` | optional | `#8a8a8a` | props, entities (placeholder box) |
| `category` | string | optional | `"Other"` | props, entities (organizational) |
| `solid` | boolean | optional | `false` | props, entities (catalog advisory; level `solid` controls collision) |
| `surface` | string | optional | none | materials/textures; `wall`/`floor`/`ceiling` documentation/validation |
| `texture` | string | **required for `material`** | — | materials. Logical id of a `texture` asset. Not allowed on other types. |
| `baseboard` | string | optional | — | materials. Logical id of a `material` asset used as the automatic trim for wall faces finished with this material (section 10, *Automatic trim*). Must resolve to a declared material; a present-but-blank value is a catalog error. |
| `tile_metres` | number | optional | `2.0` (`0.05`–`64`) | materials only |
| `grid_metres` | number | optional | the material's `tile_metres` | materials only, `0.05`–`64`. World-space spacing of the visible panel joints when a sheet paints several panels per repeat; grid-aligned ceiling fixtures snap to its cell centres (section 21). The office ceiling declares `1.0` inside its 2 m tile. |
| `tint` | `[r,g,b]` | optional | `[1,1,1]` (channels `0`–`1`) | materials only |
| `emissive` | `[r,g,b]` | optional | — | materials (`0`–`1` each; the anchor of the emission group) |
| `emissive_intensity` | number | optional | `1.0` | materials (`0`–`8`; requires `emissive`) |
| `emissive_mask` | string | optional | — | materials (logical file-backed texture id; requires `emissive`) |
| `normal_texture` | string | optional | — | materials. Logical texture id of a tangent-space normal map. |
| `normal_strength` | number | optional | `1.0` | materials (`0`–`2`; requires `normal_texture`) |
| `specular` | number | optional | `0.0` | materials (`0`–`1`): sheen strength |
| `specular_color` | `[r,g,b]` | optional | white | materials (`0`–`1` each; does not require `specular`, and has no visible effect without it) |
| `shine` | number | optional | `0.4` | materials (`0`–`1`): `0` matte, `0.5` semi-gloss, `1` extremely glossy. Not a mirror. |
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
follow exactly the same rule, and a material's `baseboard` must name a declared
`material` asset. A material may be declared before the
texture it draws with, but never with a dangling reference: the catalog does a second
pass after all entries exist, and any failure rejects the catalog.

`assets` is the one catalog collection: every entry declares `asset_class` and
`asset_type`, and duplicate ids are a catalog error.

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
| `prop` | Three-dimensional object | `model` = `.glb` under `assets/` | **Yes** — `props[].model` | levels |
| `entity` | A placeable character: a skinned GLB, posed every frame by the character path | `model` = `.glb` | **Yes** — same prop pipeline | levels |
| `material` | Surface appearance definition | `source: "definition"`, no file; names a `texture` | No | `defaults`, rooms, walls/`faces`, patches, regions, opening `glass` |
| `texture` | A surface PNG | `model` = `.png` | No | a `material`'s `texture`, `emissive_mask`, `normal_texture` |
| `light` | A fixture's visible face PNG (the fixture's mesh family is code) | `model` = `.png` | No (levels name it in `ceiling_lights[].fixture`) | level fixture ids; `src/lighting/tuning.rs` fixture table |
| `decal` | A surface marking sheet | `model` = `.png` (`source: "file"`), including the fixed internal validation atlas | No (levels name it in `decals[].material`) | level decal placement |

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
* `doubleSided` is retained with the glTF default `false`. Eligible opaque
  dynamic and animated entity primitives honor it in scene and emission passes.
  `true`, MASK foliage and BLEND surfaces keep both sides. Reflected/singular
  rigs, scaling clips and morph targets remain conservatively two-sided.
  Prepared static prop records do not carry this field and retain the existing
  two-sided contract; no current static shipped prop opts into a single side.
* Embedded PNG images only, one decoded copy per distinct image actually used; no
  external `.bin`, no external/data-URI textures, no Draco/WebP extensions.
* UVs must be finite and inside `-0.01..=1.01` — props use **non-tiling** UVs.
* **Skins** (at most one per model, on one mesh node): `JOINTS_0` (8/16-bit),
  `WEIGHTS_0` (float32 or normalised 8/16-bit), a retained node hierarchy,
  `joints` and `inverseBindMatrices`. A **placed** skinned model is re-posed by the
  character path. Fully claimed models are excluded from immutable bind-pose
  lighting casters and receive live entity grounding; partial or over-budget
  groups retain the visible static fallback's caster. Collision is unchanged.
  The engine ceiling is 128 joints per model.
* **Animations** (optional): LINEAR and STEP samplers driving node translation,
  rotation or scale; up to 64 clips and 4096 channels per model. CUBICSPLINE
  samplers and morph-target weight channels are rejected by name. A clip whose
  name contains `idle`, `walk`/`run`, `jump`/`air`/`fall` or `swim`
  (case-insensitive) is assigned to that locomotion state; states without a
  matching clip fall back to the idle clip.
* A skinned model with **no clips** is posed by the built-in procedural
  locomotion driver: leg pairs in a diagonal gait, a tail chain and the body
  chain are classified by joint name. A rig the driver cannot classify stays in
  its rest pose; entities are not required to ship clips.
* Still rejected (each with a descriptive message): morph targets, sparse
  accessors, non-triangle primitive modes, and any extension other than
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
| Prop pack decoded memory | 64 MiB (`PROP_TEXTURE_PACK_BUDGET_BYTES`); the audited 134-model library is approximately 24 MiB before runtime sharing/downsampling; `tools/assets/audit.py` reports the current inventory |
| Materials per prop | one per primitive; a multi-material model costs one draw range per material per batch |
| Distinct models per level | 4096 (`MAX_LEVEL_PROP_MODELS`; fallback boxes beyond it) |
| Summed baked prop vertices per level | 24 000 000 (`MAX_LEVEL_PROP_VERTICES`; fallback boxes beyond it) |

The GLB tools in `tools/props/` emit and enforce the art budget; follow it. A model
above the art budget may still load if the engine ceiling allows it, but it does not
match the project's visual language and the shipped-asset tests will flag it. The
budget helpers and the preview/build commands are documented in `tools/props/README.md`.

### Fallback behavior

Unknown catalog id → placeholder box (neutral size fallback `[0.6, 0.9, 0.6]` m, but
the catalog `size` is used for the placeholder when the level does not author one).
Missing/malformed GLB, over-budget mesh, more than 4096 distinct models, or exhausting
the level prop-vertex budget → placeholder box plus a one-time `[props]` warning where
a file was involved. `solid` is never affected by any of this.


### Rigid animated props (clips without a skin)

A model may declare `animations` and a node hierarchy **without** a `skins`
array. It then parses as a *rigid* animated prop: every primitive is bound to
the node that carries it with weight one, and the character path poses those
nodes directly (no joint weights, no skin). `home:wall_switch` is the shipped
example — a plate node, a pivot node and a rocker node with one LINEAR rotation
clip named `toggle` whose first key is the bind pose and whose last key is the
other position.

A rigid prop is driven **only by an explicit action**: it never runs the
locomotion states, so an untouched switch holds its bind pose instead of
looping its clip. Use `toggle_animation` (section 29) to ease its clip to
either end. A model is claimed as a character, so it does not occlude the bake
and it counts against the level's animated-character budget
(`MAX_CHARACTERS`, 128 per level).

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
| `align` | enum | no | `"none"` | `none` keeps the authored centre and rotation. `ceiling_grid` snaps a **ceiling** decal's centre to the nearest panel centre of the ceiling material above it, in the room's own ceiling tile frame, and composes that frame's rotation into the decal's in-plane rotation. The panel module is the material's `grid_metres`, falling back to `tile_metres` when the sheet paints one panel per repeat. Ignored on every other surface, and on a decal that is not inside a room or whose ceiling material resolves no positive period. |

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
* A catalog decal sheet may add `"alpha_mode": "blend"` instead of the default
  cut-out. A blended sheet is drawn by a second decal pipeline (same depth bias,
  `LessEqual` testing, but blending on and depth writes off) and is sorted back to
  front, so a soft-edged feather fades across a surface edge instead of testing at
  0.5. This is what the outdoor path-edge strips use; `"cutout"`/`"opaque"` on the
  entry keeps the historical hard cut-out, and a decal with no `alpha_mode` is
  unchanged. See §34.3.

Use a decal when you need a **local marking on an existing surface**: signs, arrows,
hazard bands, stains that must be a specific shape. Use a material change (patch or
region) when the whole surface area changes appearance, and use geometry when the
object has depth.

**Ceiling vents.** `core:decal_ceiling_vent_01` is a low-poly ceiling grille sheet
(0.6 m square) meant for `surface: "ceiling"` with `align: "ceiling_grid"`. It
snaps to one ceiling panel and composes the room's tile rotation, so it sits
square on the drawn pattern instead of straddling a T-bar. Its footprint must
fit inside a panel: on a sheet whose visible panel module is 1 m, keep the sheet
at or under ~0.8 m; on a 2 m module it can be larger. A vent is ordinary decal
artwork — alpha cut-out, no emission, no light — and never changes the ceiling
geometry.

**Hanging paintings.** The Home's west wall pairs
`core:painting_frame_landscape_01` with
`core:decal_temptation_adam_eve_01`, named "The Temptation of Adam and Eve".
The timber GLB provides depth; the separate decal preserves the supplied
900×546 photograph under High's 1024-pixel decal budget rather than the
256-pixel prop budget. Its 1024-square transparent sheet fits a 2.048 m square
quad, leaving 1.800×1.092 m of visible artwork. Keep the frame and decal
placements together; see [Asset Specification §7.1](ASSET_SPECIFICATION.md#71-framed-hanging-paintings)
for the wall-plane offset, orientation and scaling contract. The demo's
frame instance exposes the title through the existing E/toggle-label binding.

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

The compiler prepares
per-texel lightmap atlases and the baked-vertex fallback. Authors control lighting
with fixtures, prop lights and global directional illuminators; material emission
does not create illumination.
Surface response, emission, reflections and post-processing are added on top of
that bake; none of them is a light source (see section 11). Entities also receive
bounded live selected/switchable direct sources and attached `glow` lights through
the shared runtime path. Static atlases remain baked; there are no realtime shadow
maps or per-frame diffuse transport solves.

Prepared static transport uses the drawn geometry and material coverage.
Offline compilation resolves source PNGs for the solve as well as the capture;
architectural diffuse reflectance uses the mean decoded linear colour, while
the saved incident light remains independent of the receiver's displayed albedo.
An opaque surface blocks even when its PNG alpha is zero. A cutout surface
blocks where numeric level-zero bilinear texture alpha × vertex alpha × material
opacity is at or above the cutoff; holes transmit. A blended surface attenuates
straight rays by `1 - coverage`, multiplying across separate layers. It has no
refraction, coloured transmission, or diffuse bounce of its own. Architectural
UVs repeat and model UVs clamp, matching their renderer routes. Water continues
to use its authored volume attenuation independently of its visible alpha.
An open window or door is a real geometric aperture; glass is an attenuating
surface in that aperture. `solid` governs collision separately. The historical
vertex/fast-occluder fallback still uses coarse boxes, so foliage's
`occludes: false` convention remains useful there; it does not suppress actual
opaque or covered cutout triangles in the prepared solve.

Broad finite sources integrate each tap's cosine and directional moment before
averaging. Their authored size controls penumbra; brightness, range and shape
remain ordinary source parameters. Diffuse orders gather the previous order's
linear incident energy times the emitting surface's linear albedo; stored
lightmaps contain incident lighting, and the runtime multiplies receiver albedo
once. The existing quality budgets and convergence controls are unchanged.

Lightmap density describes intervals per metre: a finite positive chart span
uses `ceil(span × density) + 1` inclusive endpoint samples, with the established
page cap. Padding dilates each chart's own edge values. Receiver footprints use
a canonical world-space tangent frame and the requested physical density,
independent of chart UV winding or triangle axes. Footprints stop at real
boundaries and may cross connected, compatible coplanar construction; they do
not cross gaps, shading-normal changes or different authored source materials.
Triangle-centroid texture colour is reflectance, not material identity. Untagged
analytical scenes retain the conservative albedo boundary guard. Only diffuse
gather energy is filtered across compatible joins; direct visibility uses a
complete bounded physical coverage stencil. No authored overlap or extra seam strip
is required to make neighbouring coplanar geometry continuous.

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

### Prepared HDR lighting (medium and full atlases)

When a map is compiled with lightmaps `medium` or `full`, its illumination is
solved **offline** by the transport solver: direct emitter sampling with soft
shadows, one or two real diffuse bounces, a moment per-texel encoding
(an irradiance mean plus the signed vector sum of the per-channel moments, so
neighbouring texels always interpolate smoothly) and a prepared irradiance
field for moving objects. The stored values are linear HDR; authored colours
and brightness still mean what they meant, and the surface albedo multiplies
the result exactly once. A `switchable` fixture's contribution is prepared as
its own selectable layer, so a toggle changes real illumination without a
rebake. See `docs/RENDERER.md` §7.1 for the encoding and
`docs/PACKAGE_FORMAT.md` §5–6.1 for the payloads.

The vertex-lit model below is the exact contract of the `off` variant and of
every fallback after a plan/fill failure; it remains fully supported and its
examples still apply to a `Lightmaps: Off` map.

Two prepared-path rules keep a solved map readable, and both are automatic:

* **Water and translucent panes have distinct transmission.** A `water` volume's surface does
  not block the bake: a fixture above the pool reaches the basin, and the light
  that passes into the water is attenuated with depth — a bounded per-channel
  Beer–Lambert falloff, red absorbed most, so a deep basin reads blue-green
  while the shallow end stays brighter. A blended pane attenuates by its
  actual alpha coverage, while a cutout grille blocks its covered slats and
  transmits through its holes. Neither is equivalent to an empty opening.
  Opaque material always blocks. Collision's `solid` flag does not select
  transport alpha semantics; the visible surface still follows section 10.
* **The authored room fill is continuous.** After transport, floors, walls,
  skirts, ceilings and probes receive the receiver-local response
  `L + T * max(1 - L/(4*T), 0)^2`, where `T` is the room-area baseline above
  ambient, weighted by **visible** always-on fixture support and water attenuation.
  Zero targets remain dark. Switchable layers receive no permanent fill.
  No chart mean enters the response, so changing chart boundaries cannot
  change illumination. This is calibrated artistic fill, not an extra bounce.
* **Diffuse energy is integrated before encoding.** Each receiver sums
  `weight * max(dot(direction, normal), 0)` before compressing directions.
  The stored directional response is calibrated to that exact integral at
  the receiving normal (geometric for architecture, imported/interpolated for
  models). Visibility origins stay on the geometric side. Compression alone is not an irradiance integral:
  opposing grazing sources must not manufacture illumination. Each bounce
  transports only the previous order; two bounces mean `D + KD + K²D`.
  Current solver revision 17 invalidates earlier atlas and package fingerprints; sky is
  injected only into the first diffuse order. Cache interpolation also tests
  visibility so one floor triangle spanning a divider cannot transfer light
  through that divider.
  An encoding mismatch aborts compilation with the room, surface and texel
  diagnostic; it is not hidden by a vertex-lighting fallback.

The probe field uses 64 shared antipodal sphere samples, including the authored
sky on escaping rays. Compiler validation excludes non-air and embedded probes
and reports rooms with no valid coverage. Solver 16 keeps the centred grid when
all rooms have valid air support; otherwise it tests at most 63 additional
quarter-cell phases at unchanged spacing/caps. A phase must recover room coverage
without losing any originally covered room. All bounded phases are scored by
room coverage, then balanced per-room valid-probe counts so a boundary singleton
does not stop the search. The same chosen grid supplies target positions and
baked coefficients. Unrecoverable spaces still produce warnings;
runtime support and opaque visibility remain unchanged. `PLACES_PROBE_DUMP_DIR=<directory>`
enables deterministic JSON diagnostics; use `--force` and a separate directory
per source/build. See [Probe baker audit](PROBE_BAKER_AUDIT.md) and
[the PLPF contract](PACKAGE_FORMAT.md#61-irradiance-field-for-moving-objects).

Current PLPF v3 keeps combined light and a selected local-direct sidecar. Entities
subtract that selected energy before reconstructing indirect response and evaluate
the corresponding finite sources at their actual transformed surfaces. Eight
bounds anchors retain spatial response and tested visibility; they do not replace
the static atlas. Every new field preserves this v3 path, including zero selected
source fields with aligned zero direct coefficients. Legacy v2 keeps its combined
centre sample. Texture resolution
and filtering remain independent of live lighting quality. The current controls,
limits and capture workflow are in [Stage 4](art-style/stage4/contracts.md).

Use `PLACES_VERBOSE=1` with the compiler to log area-weighted direct, bounced,
filtered and filled measurements by room and surface family. `shoulder_fraction`
reports the area above 1.4 display-light units, where the existing soft shoulder
retains less than 5% of the input lighting gradient. It is a diagnostic, not an
artistic pass/fail threshold; fixture emission is a separate surface term.
`tools/bench/capture_lighting_regions.py` inventories source/package copies and
captures room grids, tall ceilings, raised floors and water regions with pinned
quality presets, package hashes, logs and loading traces.

### Global directional illuminators

Source format 3 accepts a bounded `global_illuminators` list (maximum eight):

```json
"global_illuminators": [{
  "id": "moon",
  "kind": "directional",
  "direction": [-0.36, -0.8, -0.48],
  "color": [0.85, 0.9, 1.0],
  "intensity": 0.12,
  "enabled": true,
  "cast_shadows": true,
  "bake": true,
  "angular_size_degrees": 0.5
}]
```

`direction` describes light travelling from the source into the world; negative
Y places the source overhead. It must be finite and nonzero, and is normalized
once. IDs must be valid and unique within this list. `color` is linear RGB in
`0..1`; intensity is `0..8`. The full angular diameter is `0..10` degrees (zero
is a hard source). Defaults are directional, white, intensity 1, enabled, shadow
casting, participating in the bake, and diameter 0.5 degrees. An inactive or
unbaked entry contributes no light; there is no realtime directional-light path.

Directional sources have no radius or distance attenuation. Their visibility
rays travel toward infinity against the same two-sided opaque geometry used by
local lights. Water extinction and transparent-pane/cutout transmission retain
their existing contracts. Medium/High store the direct source and its diffuse
bounces in the base atlas and moving-object probes; Low stores the occluded
cosine response in static vertex lighting. There is no added per-frame shadow
pass or global-light shader loop. Keep `sky.ambient` separate: it is the existing
diffuse dome contribution on escaping gather rays, not a directional source.
Lantern Hollow's generator authors the example moon above without increasing
its sky ambient; its star sheet contains no visible moon.

### Sampling quality and diagnostics

Gameplay Low uses vertex lighting. Medium uses nominal 12 texels/m for
architecture, two emitter taps per axis, one diffuse order and 32 gather
directions; High uses 16 texels/m, three emitter taps, two orders and 64 gather
directions. Small opaque model charts use twice the architecture density:
24 intervals/m for Medium and 32 for High. Large opaque models retain one eighth
of architectural density; cutout charts retain 1 interval/m. Compatible coplanar
triangles in one original model primitive share a native four-corner chart.
The original common diagonal, all six drawn corners, material UVs and normal
frames remain exact; incompatible unions retain individual triangle charts.
Actual endpoint-grid density is `(chart_axis_texels - 1) / axis_metres`.
Medium integrates every 2×2 receiver-footprint sample; High integrates every
4×4 sample. Equal centre/corner visibility does not establish uniform coverage.
Coplanar chart joins compare
supported neighbours in world space. Only diffuse gather energy is denoised;
its filter crosses a chart join only with matching normals/material and an
unoccluded connection. Atlas endpoints address texel centres, and the existing
one/two-texel gutters cover the single-mip bilinear runtime footprint.

`PLACES_LIGHTING_DUMP_DIR=<directory>` writes stage atlas PNGs, linear
`*.rgb-f32le` buffers and chart metadata during an explicit compiler `--force`
build. Use one directory per source and variant. Stages include direct, total
bounced, indirect, filtered, filled, global direct, geometric and shading normals.
`PLACES_LIGHTING_LOCAL=<emitter index>` adds one local emitter's isolated direct
field. `emitters.json` lists emitter indices and positions.
`PLACES_LIGHTING_RAYS=<JSON file>` accepts up to 4096 finite rays:
`[{"origin":[0,1,0],"direction":[0,1,0],"max_distance":10}]` (distance optional).
The report names each first opaque blocking triangle, its architecture/model
range, corners, normal and distance. Ray inputs are validated before output.
These exports do not alter the physical solve. There is no separate AO or
runtime shadow-map stage to isolate.

`python3 tools/levels/build_lighting_quality.py --check` verifies the deterministic
fixture/camera source. `tools/bench/capture_lighting_quality.py` captures those
controls and fixed production views, or benchmarks the same cameras with
`--frames`. A preserved binary/package directory can be selected for A/B runs.
The capture guard rejects a failed requested level even when the player falls
back to the demo. `tools/bench/inspect_lighting_dump.py` projects a selected
room/surface stage into world coordinates; `--stage chart-ids` and
`--stage density` visualize chart ownership and actual endpoint density.
It uses optional NumPy/Pillow analysis tools, outside the game's dependencies.
`--audit --out <audit.json>` validates finite nondegenerate chart geometry,
unit geometric normals and disjoint padded atlas reservations. It also reports
one-sample axes and extreme world-axis ratios so small/elongated charts can be
investigated without silently treating deliberate low-density cards as errors.
Omit `--room` to inspect models; `--normal`, `--plane` and `--bounds` isolate
a particular oriented face. Triangle and trapezoid projections follow the
mesh's piecewise affine UVs. `--stage indirect` subtracts direct from bounced
energy, and `--legacy-uv` reconstructs the previous runtime sample positions
for an unchanged baseline export.

For native device timing, attach Apple's Metal System Trace after the requested
map has loaded. Export the `metal-gpu-intervals` and
`metal-current-allocated-size` tables using `xctrace export`, then pass their XML
files to `tools/bench/inspect_metal_trace.py --gpu <gpu.xml> --memory <memory.xml>
--out <summary.json>`. Its stable window defaults to trace seconds 1 through 7.
It reports the union of active Places GPU intervals per scene encoder, avoiding
double counting simultaneous vertex/fragment work, and the measured Metal
allocation. This is device occupancy per rendered frame; CPU `render_ms` only
measures submission. Profile idle fixed-camera runs separately from compiler
jobs and diagnostic capture/export work.

### The implemented vertex-lit lighting model

Each local light is a generic engine-level source: a **shape** (point, rectangle, line), a
world position, an RGB colour, an intensity, a **range**, a **falloff** curve and an
`enabled` flag. Visible fixtures and placed props are ordinary objects that *own* zero
or more of these sources. Adding a new glowing object never means adding a new light
family.

```text
sample = clamp(AMBIENT_LEVEL + room/partition-area baseline
       + visibility-tested direct pools
       + visibility-tested bounce fill
       + doorway-blend deltas, 0.10, 1.0)
```

1. **Room baseline.** Each room sums `intensity × height factor × colour` over the
   fixtures it owns, spreads it over its floor area, compresses the density and maps
   it onto `[AMBIENT_LEVEL, BASELINE_MAX = 0.52]`. The baseline is deliberately the
   *fill* level, not the highlight: it is the one term a static occluder cannot
   remove, so it leaves the visibility-tested direct pools room to read as light and
   shadow instead of pinning every surface at the clamp. A room with no fixtures sits
   at exactly the ambient `0.10`: unlit rooms are dark by design.
2. **Partitions.** If opaque internal walls split a room's footprint into
   disconnected areas, each area gets **its own baseline** from the fixtures it can
   reach. A wall that stops short of the ceiling is not a partition; a door header
   separates while a doorway keeps a bounded blend; a window does not connect
   baselines at all. The connectivity probe runs 0.15 m below the ceiling, so a wall
   must cross more than 0.75 m into the room before it is a partition candidate.
3. **Local fixture pools.** Each fixture adds a bounded local pool on top of the
   fill. A **ceiling fixture's** pool is directional: it falls with the horizontal
   distance from the emitting rectangle (`(1 - d/range)²`), is weighted by the
   cosine between the surface's vertical and the emitter (`vertical / distance`), and
   only reaches samples *below* the emitter. The floor directly beneath a panel is
   the local maximum; the ceiling the panel is recessed into receives no direct
   light. **Wall sconces and prop-attached lights** keep the isotropic
   `brightness × height factor × falloff` ball. Pools are capped at `0.45` per
   channel by a screen composition, not a hard sum: a single fixture below the cap
   adds its full energy, overlaps grow monotonically without flattening to grey, and
   a warm fixture keeps its colour to the cap.
4. **Bounce fill.** Every light also adds a broad, weak second pool (cap `0.26`)
   out to 1.5 times its `range`, visibility-tested like the direct pool. This is the
   room's first reflected light: it is what lights the ceiling around a recessed
   fixture and the upper walls a downward pool does not point at, without inventing
   a source or a global ceiling override.
5. **Wall-boundary occlusion.** A pool only reaches what its fixture can see: light
   is tested against the exact wall solids. Doors, windows, passages and vents all
   transmit through exactly the hole they cut; a solid header/sill still blocks.
6. **Doorway baseline transfer.** Only `door` and `passage` openings whose bottom
   reaches the lower connected floor blend a bounded amount of the neighbour's
   baseline through the aperture (radius 6 m, strength 0.5, fading over 1 m above the
   header). Windows and vents transmit pools only; they never blend baselines.
7. **Vertical isolation.** Floors and ceilings are light boundaries: stacked rooms do
   not light each other through a slab, in brightness or colour. A raised platform or
   lowered basin inside one room volume is not a barrier, and an open side of an upper
   floor transmits normally. When rooms share a footprint, author a light `y` to pick
   the storey. A ceiling fixture's direct pool reaches the floor directly beneath it
   whatever the ceiling height (height only enters through the ceiling-height factor),
   so a tall space is not blacked out by the pool's radius.
8. **No ambient control.** The 0.10 neutral floor is fixed; you cannot author sun,
   sky, a room-wide brightness or a room-wide tint. Express mood per fixture.

### Baked lightmaps

The compiler stores linear incident irradiance and signed moments per texel of
static geometry, including panes, water and real static model triangles. The
player uploads these prepared package layers and multiplies receiver albedo once.

* Independent Lightmaps **Full** uses 16 texels/m and **Medium** 12 texels/m,
  both with 1024-pixel pages. Full plans up to **ten base pages** per contribution
  group; Medium retains **eight**. **Off** uses the historical vertex-lit representation. High/Medium/Low
  presets select Full/Medium/Off; advanced choices may override them.
  The shared chart-span cap is 63.6875 m, preserving room for inclusive endpoints
  and gutters. The deterministic best-short-side-fit MaxRects allocator reports
  named page overflow rather than dropping surfaces. Each resident page uses two
  RGBA16F layers; every switchable group adds another pair per base page. The GPU
  allocates exactly the produced layers, without reserving unused pages.
  [The Stage 7 Hallows capacity measurement](art-style/stage7/integration-contract-audit.md)
  finds 9,549,535 padded Full texels, exceeding nine-page capacity; the existing
  packer reaches the mathematical minimum of ten. This costs 160 MiB of base
  RGBA16F GPU storage at Full, compared with the prior eight-page 128 MiB policy.
  Density, endpoint samples, gutters, chart orientation and content are retained.
  Decoder page/byte, switch-group and aggregate safety bounds are unchanged.
  The separate model-lighting correction now shares compatible native prop
  charts and uses finer small-opaque sampling (High32/m, Medium24/m). Its
  Hallows Full plan needs eleven pages: 176 MiB for the base group. The
  320 MiB +64 KiB typed record, group/shape and aggregate bounds remain enforced;
  extra page capacity does not waive them. See
  [the correction report](model-lighting-root-cause-and-fix.md) for current
  verification status, distinct from the dated Stage 7 evidence.
* Medium/Full finite-source transport uses the selected solver's shaped taps
  with per-tap visibility/cosine and complete bounded receiver coverage. The
  historical vertex fallback retains its centre-only coarse-box shadow path.
  The legacy profile bake API's five-tap quincunx is not the complete prepared
  transport solver contract; see [Prepared HDR lighting](#prepared-hdr-lighting-medium-and-full-atlases).
* A chart's texels span their own patch: the first and last texel sit exactly on the
  patch's geometric edges. Adjacent coplanar surfaces therefore evaluate the *same*
  world point on a shared edge, so changing an albedo material across one continuous
  floor changes the texture and leaves the baked illumination continuous. Charts stay
  separate wherever the lighting is genuinely discontinuous (a 90-degree corner, a
  wall, a different room).
* A wall chart resolves its room **per texel**, not once per emitted strip. Abutting
  wall pieces coalesce into emission units, and one unit's run can cross a room
  boundary (two rooms sharing a corridor wall, a run through a doorway); resolving a
  single room for the whole run would switch the baked light at the arbitrary run seam
  rather than at the room boundary and step the brightness of a continuous face.
  Floors and ceilings are emitted per room and keep their exact hint.
* An interface plane (a room's floor) treats a sample within one millimetre of itself
  as *on* the plane, not across it: the mesh reaches a wall base's height through a
  different float expression than the plane's own `y`, so the two can differ by a
  ULP, and without the contact band the floor shadowed its own wall base and two
  coplanar strips whose bottoms rounded opposite ways stepped at the seam.
* `settings.json` carries `"lightmaps": "off"|"medium"|"full"` (default
  `"full"`), exposed as
  Settings → Graphics → Lightmaps and switched live by installing a prepared
  package variant, with player state preserved. The previous resident world
  remains installed until staged resources commit. The
  environment override `PLACES_NO_LIGHTMAPS=1` forces the historical vertex-lit
  path for a benchmark or A/B capture run.
* If a bake cannot fit the page budget, or an atlas page cannot upload, the level
  rebuilds with `LightmapMode::Off` and draws exactly the old vertex-lit colours — a
  level never renders black because of a lightmap failure. A single quad with no
  usable area (a sub-millimetre trim sliver) is *not* such a failure: it is invisible,
  so the plan leaves that quad vertex-lit and reports it in the `[lightmaps]` line
  (`left N sub-texel sliver quad(s) vertex-lit`) while the rest of the level keeps its
  atlas. A visible malformed quad (a bow-tie) still fails the build over.
* Fixtures, prop placeholder boxes and decals retain the vertex-lit path.
  Panes, stairs, water and real static props receive prepared charts in
  Medium/Full. Movable objects use spatial residual probes plus bounded live
  finite-source direct light, with the legacy centre path for genuine PLPF v2.
* Set `PLACES_DUMP_LIGHTMAPS=1` to write the baked atlas pages as PNGs under
  `target/diagnostics/atlases/` for inspection.

#### Static props occlude the bake

A placed prop is not air: prepared transport uses its actual triangles and
numeric material/PNG coverage. The vertex/fast-occluder fallback derives a small
set of boxes per distinct model unless the placement authors `occludes: false`.
Those fallback boxes are ground on a
quality-level grid — **High** 0.075 m, **Medium** 0.11 m, **Low** the historical
0.15 m — so High resolves a finer contact silhouette and the shadow a prop throws
on the floor or wall behind it, while Medium and Low keep cheaper derivations. The
visible consequences:

* the floor under a machine, desk or couch is darker than open floor at the same
  distance from a fixture (contact darkening);
* a large object blocks the pool behind it — a fridge or vending machine throws a
  real shadow onto the wall and floor behind it;
* furniture pushed into a corner darkens that corner, so props read as standing
  *in* the room rather than pasted onto it;
* a rotated prop shades along its rotation, not along its bounding box;
* a prop's own `lights[]` still cast normally, and are themselves blocked by the
  prop body.

Two consequences matter when placing props: a fixture never lights straight
through a machine, and a prop that
is *not* solid still occludes light (occlusion follows the drawn model, not the
collision box).

#### Dynamic objects

The engine has a separate render path for objects whose transform changes every
frame — moving components that must not be re-baked, re-batched or written into
the static lightmap. A level authors its own moving solids as doors: see
[§30](#30-doors-switches-and-effects). The path is also proven by one generated
object: a `core:washer_drum` turning inside every placed `core:washing_machine`,
behind its open porthole and inside the machine's cavity (the machine itself is
an ordinary static prop and participates in the bake).

* **Door frames and leaves** are authored in level JSON and both draw through
  the dynamic path; the leaf carries the collision and is the level's only
  map-authored movable solid.
* The **washer-drum demonstration** is engine-created: placing a
  `core:washing_machine` is the only way a level influences one.
* Movable receivers use eight inset bind-bounds support anchors for residual
  irradiance, and selected finite-source direct light at real fragments. Rigid
  moving casters use cached triangle/alpha visibility; skinned floor contacts
  use bounds proxies. Genuine legacy PLPF v2 retains one centre probe.
* Moving one never rebuilds geometry, batches or lightmaps, and a door leaf casts
  no baked shadow.

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
| `core:fluorescent_panel_01` | ceiling (default) | `environment/office/textures/lights/fluorescent_panel_01.png` (1024×512) | Recessed troffer, outer footprint 1.2 × 0.6 m (half extents 0.6 × 0.3); rotation swaps axes | The default family **and the fallback for every unknown id**. A real housing frames the diffuser: side walls drop 0.045 m from the ceiling plane, a bottom frame borders the aperture, a top flange closes the body, and the fitted 2:1 sheet (aperture 1.12 × 0.56 m) is recessed 0.012 m above the frame's bottom. Only the diffuser glows; the housing is a fixed mid grey. Hangs 0.01 m below the local ceiling; under a gable it follows the eave/ceiling above its footprint. Grid alignment (section 21) snaps its centre to the ceiling material's panel grid at load by default. |
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
   geometry (a can, a housing, a frame) into `housing`. For an example, the office
   panel emits its diffuser into `lit` and a real troffer housing (side walls,
   bottom frame, top flange) into `housing`.
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
8. Validate: `python3 tools/textures/build.py --check`,
   `python3 tools/assets/validate.py`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`.

Restyling the panel family means adding a catalog fixture entry with its own PNG,
exactly like the shipped families; fixture faces resolve from catalog `light`
entries by fixture id.

---

## 19. Prop Placement

Exact level syntax (all fields verified against `src/level.rs::PropDef`):

```jsonc
{
  "id": "front_desk",            // optional. Stable per-instance id; default <model>_<n>.
  "display_name": "Front Desk",  // optional. toggle_label text; default the model id.
  "model": "core:desk",          // REQUIRED. Logical catalog id. Unknown ids render a placeholder box.
  "x": 4.6, "y": 0.0, "z": 5.8,  // optional, default 0. y is an offset ABOVE the local walkable floor.
  "rotation_degrees": 180.0,     // optional Y rotation; model +Z faces this way at 0.
  "scale": 1.0,                  // optional, default 1.0, must be > 0. Scales model and explicit size.
  "size": [1.6, 0.75, 0.7],      // optional [w,h,d] metres for collision/placeholder.
  "solid": true,                 // optional, default false. Only this flag creates collision.
  "occludes": true,              // optional, default true. Set false for alpha-cutout scenery (grass).
  "components": [                // optional typed capabilities (see §29).
    { "component": "interactable",   // an E-aimable object.
      "prompt": "Toggle name",       // optional; shown while aimed at; default "Interact".
      "reach": 2.5,                  // optional; 0..4.0 m; default 2.5.
      "enabled": true,               // optional; false starts it aimed-but-disabled.
      "label": null },               // optional; display name a toggle_label may show.
    { "component": "state", "name": "phase", "value": "cold" },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [                  // optional event wiring (see §29).
    { "on": "interact",          // closed event-kind enum.
      "when": [],                // optional conditions; all must hold.
      "once": false,             // optional; default false.
      "cooldown_seconds": 0.0,   // optional; default 0.0.
      "actions": [{ "action": "toggle_label" }] }  // REQUIRED, 1..8, in order.
  ],
  "lights": [],                  // optional 0..8 generic light sources, see section 21.
  "float": { ... }               // optional water-driven motion, see the field table.
}
```

Semantics:

* **`id` is a per-placement identity, not an asset id.** It names *this* placed
  instance for bindings, action targets and duplicate validation, and has
  nothing to do with `model`/catalog ids. Omitted, the deterministic default is
  `<model short name>_<n>` where `n` counts the placements of that short name that
  do not author an id, in array order. One **instance namespace** covers props,
  doors, ceiling fixtures, trigger volumes, timers and spawn points; a duplicate
  or malformed id is a named load error. Sequence, spawn-template and
  spawn-group ids are separate resource namespaces. Never key
  external state on `model`: two copies of one model are two instances.
* `display_name` is the floating label text a `toggle_label` action shows. It is
  map-authored; the model id is the fallback when omitted.
* `y` is **not absolute world Y**: `base_y = walkable floor at (x,z) + y`. A negative
  `y` deliberately sinks a prop into the floor and is never corrected. On a room with
  `floor_y: -1.5`, a prop at `y: 0` stands on that room's floor.
* **Collision box = level `size` (or `PROP_FALLBACK_SIZE [0.6, 0.9, 0.6]`) × `scale`.**
  The catalog `size` is never used for collision. A solid prop that should block like
  its picture must author `size`. Validation rejects non-positive/non-finite `size`
  and `scale`.
* **A solid prop's box is its standable mass, not necessarily its full visual
  bounds.** The box top is the surface a jump or step lands on, so a model whose
  geometry rises above the surface a player can stand on (a sink's decorative
  faucet post above its deck, a lamp's thin finial) must author the standable
  height, not the model's bounding height: `core:sink` is 0.6 × **0.9** × 0.55 in
  the demo even though the model's faucet reaches 1.1 m. A small feature left
  outside the box is walked through rather than turned into an invisible blocker;
  a box top above the jump apex (`1.0 m`, see below) is not landable at all.
* A solid prop's box is **landable on top and blocking underneath**: the sides stop
  a walking player, a fall lands on the top (if the player's centre is over the
  box), and a jump under a raised prop bumps its underside. A prop authored above
  the floor (`y: 1.0` with a small `size[1]`) is a beam a crouched 0.9 m body fits
  under and a standing or jumping one does not.
* A **climbable ladder prop should be `solid: false`** with a matching `ladders[]`
  volume: the generic box would block the climb approach and the top exit.
* A prop that **overhangs a standable surface with less than a player's height of
  clearance** must not be solid: a 1.8 m body standing on the 0.9 m counter is
  pushed off it by the box. The demo kitchen's upper cabinets are 0.55 m above
  the counter top, so they are `solid: false` (`occludes` stays true, so the
  artwork and baked lighting are unchanged); the base cabinets already block the
  same floor footprint. Clearance for a *crouched* 0.9 m body is still clearance:
  a beam whose underside is at 1.0 m over a walkable floor is solid and a
  standing player bumps it.
* Author collision `size` in the model's **local frame**. `rotation_degrees`
  rotates its horizontal half-extents, then the collider uses the enclosing
  world-axis-aligned box (`PropDef::solid_collider`). At 90°/270° the world x/z
  extents swap automatically; do not swap the authored local size. Oblique
  rotations conservatively enclose the rotated footprint.
* Rotation also rotates the rendered model around Y.
* The **placeholder box** (unknown id, missing model or over-budget model) uses the
  level `size` when authored, else the catalog `size`, × `scale`.
* `props` are never tested against their render mesh for placement; intentional
  clipping and overlap are preserved.
* **Every placed prop occludes baked lighting**, automatically, from its rendered
  model: the floor under it darkens, it blocks the fixtures behind it, and it
  darkens the wall it stands against. `solid` controls collision only — a
  non-solid prop still occludes, because the occlusion comes from the drawn
  geometry. See [Static props occlude the bake](#static-props-occlude-the-bake).
* `"occludes": false` opts one placement out of that bake contribution without
  changing collision or rendering. The bake derives coarse solid boxes from
  triangles and cannot see alpha, so **alpha-cutout scenery must opt out**: a
  grass field or a leaf canopy left at the default would bake thousands of solid
  blade boxes into the light. `tools/levels/scatter_grass.py` writes the flag for
  every tuft it emits. A tree's trunk shadow is lost with the canopy when a whole
  placement opts out; author the choice deliberately (see §34.5). The flag
  removes the placement's derived boxes and its vertex-bake contribution; it does
  **not** remove opaque drawn triangles from the prepared transport scene
  (§18). MASK/cutout primitives transmit prepared rays using the same open-card
  approximation as cutout architecture; individual blade shadows are not traced.
  An opaque prop that must not touch the light has to sit out of the light's path
  as well — see §35.2 for a containment collider that does.

| `float` | object | no | — | Water-driven motion for a floating prop: `{ "draft": 0.03, "bob": 0.012, "bob_seconds": 2.8, "heel_degrees": 6.0, "heel_seconds": 3.6, "phase": 0.0 }`. The prop is drawn by the dynamic path on the water surface, at `surface_y - draft + bob · sin(...)`, with **no horizontal drift**; `heel_degrees` rolls it about its own forward axis. It must be `solid: false`, author `size`, sit fully inside one water volume once the swept footprint (half-diagonal plus the heel's excursion) is added, and it cannot be routed. `phase` is `0..=1`; omitted, a deterministic per-placement phase keeps two floats out of lockstep. |


Known-valid examples:

```json
{ "model": "core:desk", "x": 4.6, "y": 0.0, "z": 5.8,
  "rotation_degrees": 180.0, "size": [1.6, 0.75, 0.7], "solid": true }
```

```json
{ "model": "core:pool_guardrail_straight", "x": 14.0, "z": 9.9,
  "size": [2.0, 1.05, 0.08], "solid": true }
```

A deliberate sunken prop (from the generated prop showcase fixture; its `id` is a
fixture-generated instance id, which the engine now reads):

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

All fixtures — ceiling and wall — live in the level's `ceiling_lights` array.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `fixture` | string | **yes** | — | Fixture id from the catalog/registry. Unknown ids render and bake as the office panel with the untextured sheet. |
| `id` | string | no | `<fixture short name>_<n>` | Stable instance id for `set_light`/`toggle` actions (see §29) and duplicate validation. |
| `x`, `z` | number | **yes** | — | World position. Must be finite. |
| `rotation_degrees` | number | no | `0.0` | Y rotation. Ceiling families quantise to a 0°/90° axis swap; wall fixtures rotate continuously. |
| `brightness` | number | no | `1.0` | Must be finite and `≥ 0`; baking clamps to `8.0`. |
| `color` | `[r,g,b]` | no | `[1.0, 0.96, 0.88]` | Each channel `0`–`1`. Drives the illumination only: the visible face is texture-first and the light colour never repaints the artwork. |
| `mount` | `"ceiling"` \| `"wall"` | no | `"ceiling"` | Closed enum. Wall fixtures require `y` or the level is rejected. |
| `y` | number | no (required for wall) | derived for ceiling | Ceiling: optional mounting world Y (also selects a storey in stacked rooms). Wall: required world Y of the fixture centre. |
| `range` | number | no | `6.0` | Reach of this fixture's pools in metres; must be positive and finite; clamped to `0.05`–`64`. A ceiling fixture's direct pool falls to zero at `range` measured horizontally from its emitting rectangle; its bounce fill reaches `1.5 × range`. A shorter range is also a softer pool. |
| `falloff` | `"smooth"` \| `"linear"` \| `"constant"` | no | `"smooth"` | Closed enum. Lateral pool decay curve for a ceiling fixture (`smooth` = `(1 - d/range)²`), or the radial curve for a wall sconce or prop light. `constant` holds full strength to `range` then stops (a deliberately hard pool). |
| `enabled` | boolean | no | `true` | `false` keeps the fixture's visible glow but removes **all** of its environmental illumination. |
| `emission` | number | no | the fixture's `brightness` | Independent emissive strength of the visible face, finite, `≥ 0`, clamped to `8.0`. Lets a face read brighter (or dimmer) than the light the fixture casts. |
| `switchable` | boolean | no | `false` | Marks the fixture as externally controllable: `{ "action": "set_light", "target": "<id>", "on": false }` or `{ "action": "toggle", "target": "<id>" }` may switch it at runtime (see §29). A switchable fixture is excluded from its room's baked baseline and contributes only its local pool; each switch state is prepared as its own selectable lightmap layer, so toggling **selects a prepared layer** — a real change of the baked illumination with no runtime rebake — and the visible face turns off with it. Moving objects and characters using a newly compiled v3 irradiance field receive the fixture's live direct light through reserved runtime source slots; base probes exclude switchable energy, so it is added once. Switched diffuse bounce is absent from the entity base field. Genuine legacy v2 fields retain their combined center path without switched entity response. A fixture that is **not** switchable is baked once and never changes: no action can move its illumination. Emission-only motion (`animated_emissions[]`) never changes illumination either; it animates the face's additive glow only. Layer selection needs the prepared lightmap variants (the default): with Lightmaps Off the level bakes into vertex colours, so the face still switches but the baked illumination cannot change until the level is reloaded. Leave it `false` unless a map action drives it. |
| `align` | `"grid"` \| `"none"` | no | `"grid"` | Closed enum. Grid alignment snaps a fluorescent panel's centre onto its ceiling material's visible panel grid at load (see *Ceiling grid alignment* below). `"none"` keeps the authored `x`/`z` exactly. |

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

### Ceiling grid alignment

Ceiling artwork is a world-space tile: a ceiling material whose catalog entry
declares `tile_metres: 2.0` repeats every 2 m from the world origin. A sheet can
paint more than one panel per repeat — the office ceiling art is four 1 m
panels inside its 2 m tile — so the catalog also declares the visible panel
module as `grid_metres` (section 14), which defaults to `tile_metres` for a
sheet that paints one panel per repeat. A fluorescent panel placed by eye often
crosses the T-bar and reads as a fitting that was dropped in by accident.

By default the loader fixes that: at load, every fluorescent panel whose
`align` is `grid` snaps its centre to the nearest **panel** centre of the
ceiling material above it, on both axes. The snapped position is the one the
baked illumination, the drawn mesh, the fixture probe and collision all use —
there is no second copy. The formula is

```text
snapped = ((v - T/2) / T).round() * T + T/2
```

where `T` is the ceiling material's `grid_metres` and `v` is the authored `x`
or `z`; `f32::round` is half-away-from-zero, so a placement exactly on a panel
line moves to the higher cell. Only `x`/`z` change: rotation, colour,
brightness, range, falloff and emission are untouched.

Alignment applies only to the grid-panel family under a flat ceiling, inside a
room, with a ceiling material that resolves a positive finite `grid_metres`.
Round downlights, gable ceilings, blank or unresolved ceiling materials and
fixtures outside every room keep the authored position. Author `"align":
"none"` when a panel must stay exactly where it was placed (for example, a
deliberately skewed installation or a placement that matches a prop). An omitted
`align` gets the default (`grid`), and every panel the demo ships ends up centred
in its ceiling panels.

### The room's own ceiling tile frame

Ceiling artwork normally tiles from the world origin, but a room may author a
local phase and rotation for its ceiling pattern:

```json
{ "x": 10.0, "z": 0.0, "width": 6.0, "depth": 12.0, "height": 3.0,
  "ceiling_tile_origin": [0.75, 0.25], "ceiling_tile_rotation_degrees": 90.0 }
```

* `ceiling_tile_origin` is a world `[x, z]` point that becomes the pattern's
  local origin; omitted means the world origin.
* `ceiling_tile_rotation_degrees` rotates the pattern about that origin.
  Omitted means `0`.

The frame changes only the ceiling material's texture phase/orientation, never
the room's geometry or the floor. It is also the frame the light-fixture snap
(above) and `decals[].align: "ceiling_grid"` resolve in, so a room with an
offset or rotated module keeps its fixtures and vents on its own tiles instead
of an assumed global grid. Both fields are ignored by levels that do not author
them.

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
  "brightness": 0.38, "range": 4.5, "emission": 1.0 }
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
| `intensity` | number | no | `1.0` | Finite, `≥ 0`; clamped to `8.0` while baking. |
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
* **One map, every quality level.** High, Medium and Low run the same geometry,
  ids, materials and lights; a lower level lowers texture resolution, lightmap
  density, scene resolution and surface detail. Never author a second variant.

Water is a **volume you author** (`water[]`, see
[Water volumes](#water-volumes-wading-swimming-and-surfacing)): a translucent
surface plus wading and swimming, with no refraction and no transmission. Puddled
water that the player should not swim in can instead be implied with a wet
`floor_patches` material (`core:pool_deck_wet_01`, a near-mirror `planar` sheen over
the dry tile). Windows may hold real glass (see
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

### Doorway Sliver Off The Wall Plane

**Symptom:** a narrow wall section beside a doorway protrudes past, sits behind or
fails to meet the adjoining wall face; the junction shades or lights as a step.

**Cause:** a wall section was placed with its thickness on the wrong line — often the
min-corner placed where the centre-line belongs, so both of its faces sit half a
thickness off the wall it should continue.

**Authoring rule:** continue the same wall line. The geometry checker reports a
`wall-joint-step` error with the measured shift and the authoritative wall, and
`--repair-geometry` plus `tools/levels/repair_alignment.py --apply` moves the losing
wall (and its skirting and tucked floor edges) back onto that line. Do not fix it by
moving the main wall or by widening a threshold.

### Equal-Top Wall Corners

**Symptom:** flicker or a moiré pattern on the top of a wall where two walls meet,
seen from above in an open multi-storey space.

**Cause:** two perpendicular walls end at the same height with their top caps exposed
(their tops are below the local ceiling), so the corner square used to be emitted
twice.

**Authoring rule:** none — this is handled by the emitter. A later-authored wall's
top or bottom cap subtracts every earlier wall's footprint at the same plane, so
exactly one wall owns each cap rectangle and no hole appears. The geometry checker
still reports any residual coincident pair as `duplicate-surface`; a clean rebuild
must show zero. Authoring walls at a shared height is correct and expected.

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

### A Prop That Shadows the Light Behind It

**Symptom:** a floor or wall behind a machine/cabinet is darker than the open floor
beside it; the object looks grounded instead of floating.

**Cause:** intended. Static props occlude baked light, derived from the
rendered model: a prop standing in front of a fixture shadows what is behind it.

**Authoring rule:** it is the desired result. If a space reads
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
vertex-lit fallback always exists (`"lightmaps": "off"` in `settings.json`, or
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

**Prevention:** ids are globally unique across the catalog's `assets` entries.
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

**Cause:** extensions other than `KHR_materials_emissive_strength`, morph targets,
sparse accessors, external textures, non-triangle primitives, >65 535 vertices,
>6000 triangles, >32 primitives, >16 materials/images, >1024 joints, >64 animation
clips, >4096 animation channels, CUBICSPLINE samplers, a texture edge >1024, or a
malformed skin (a joint slot outside the skin, a non-finite or non-positive weight
sum, or an inverse-bind count that does not match the joint list).

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
| `python3 tools/assets/validate.py` | Catalog parse; classes/types/sources; unique ids; every file-backed resource exists exactly once; material texture/mask/normal references; shipped/drop-in/fixture levels reference declared ids; entity instance ids; components; event bindings, conditions and actions; trigger volumes, timers, sequences and spawn definitions; prop-light and fixture-pool shapes/fields; animated-emission schema; warns when a wall touches no room | **Yes** |
| `cargo test --workspace --all-features` | The whole Rust suite: level/loader/render/material/collision/lighting tests plus the audits (surface, lighting, isolation, parity, partition, vertical, leak) | **Yes** |
| `cargo fmt --all --check` | Rust formatting | **Yes** when code changed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Strict lints (`AGENTS.md` policy) | **Yes** when code changed |
| `python3 tools/textures/build.py --check` | Texture/decal/fixture PNGs exist, parse, ≤1024 (sky ≤2048×1024, 2:1 POT); warns >256 / non-POT | Yes when art changed |
| `python3 tools/props/build.py --check` | Every catalogued prop GLB exists and parses, and its decoded texture memory fits the per-texture and 64 MiB pack budgets; prints bounds/budget flags | Yes when props changed |
| `python3 tools/assets/audit.py --workers 12` | Every GLB and standalone PNG, finite geometry/UVs/skin data/clip channels, fitted UV range, zero-area faces, native image budget and equivalent PNG source coverage; records topology, materials, bounds, clips, duplicates and map uses in JSON. Topology review candidates require visual interpretation | Yes when assets changed |
| `./target/release/places-compile build <source>.json` | Compiles the edited source into its `.placesmap` (prepares geometry, lighting, atlas, collision and probe captures) | **Yes, after every map edit** |
| `./target/release/places-compile validate <package>.placesmap` | Decodes every prepared record and re-hashes every entry and dependency | **Yes, before shipping** |
| `PLACES_LEVEL=<id> cargo run` | Boots straight into the compiled level and prints validation errors verbatim | **Yes, once per map** |
| `PLACES_CAPTURE=frame.png PLACES_LEVEL=<id> cargo run` | One-frame PNG capture for visual inspection (`PLACES_CAPTURE_FRAME=n` waits for frame n first) | Useful |
| `cargo run --release -- --check-geometry --level <path-or-id>` | The read-only map geometry checker: confirmed defects and heuristic warnings over the engine's own geometry and collision interpretation (§31) | Recommended for every map change |
| `./target/release/places --repair-geometry --level <path-or-id> --plan <json>` | The read-only wall-joint repair planner: confirmed shifts, authority evidence, coupled edits and a post-check summary (§31) | After a `wall-joint-step` finding |
| `python3 tools/levels/repair_alignment.py --plan <json> --check` / `--apply` | Verifies and applies a wall-joint repair plan byte-faithfully and atomically; refuses a concurrent change, validates with the checker and proves idempotence | When the plan is approved |
| `python3 tests/test_package.py` | Repository/package gate: shipped-level checks, texture policy, catalog validation, README hygiene | Recommended before shipping a map into `assets/levels/` |
| `PLACES_DUMP_LIGHTMAPS=1 ./target/release/places-compile build <source>.json` | Writes the baked atlas pages as PNGs under `target/diagnostics/atlases/` (the compiler owns the bake) | Useful |
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
  faces, opening `glass`, patches, decals, fixtures, props, door material defaults
  and overrides, effect materials, animated-emission
  materials), prop lights and fixture pool/emission fields, door and effect
  field/limit checks, animated-emission schema, entity instance ids and typed
  components, event bindings/conditions/actions with resolvable capability-fit
  targets, trigger-volume/timer/sequence/spawn definitions,
  and a warning when a wall touches no room.

Only the Rust loader enforces: format version, identity edge cases, spawn
finiteness, room/wall/region/opening dimensions and bounds, floor-region
containment and eave rule, prop/light/decal/door/effect schemas, a door's
closed-leaf-versus-solid check, the geometry budgets, limits and caps, gable
rules, decal surface rules, the zero-delay cycle search, and the
walkable/collision behaviour. Boot the level to prove those.

### Environment switches

These are session switches, not authoring fields. They exist for benchmarking,
capture and diagnosis.

| Switch | Effect |
| --- | --- |
| `PLACES_LEVEL=<id>` | Boot straight into a level and print its validation errors. |
| `PLACES_QUALITY=low\|medium\|high` | Draw this run at the named quality level without editing `settings.json`, so the same map can be captured at different levels back to back. The Settings screen shows the overridden level (marked `*`) and changing it there clears the override. |
| `PLACES_CAPTURE=<file.png>` | Write one frame as a PNG and exit. |
| `PLACES_CAPTURE_FRAME=<n>` | Capture frame n (1-based) instead of the first; also pins animation phase. |
| `PLACES_NO_LIGHTMAPS=1` | Force the historical vertex-lit path for this run (overrides the Lightmaps setting). |
| `PLACES_DUMP_LIGHTMAPS=1` | Write the baked atlas pages to `target/diagnostics/atlases/`. |
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
- [ ] New props that shadow a lit area are intentional; the space still
      reads with the contact darkening.

### Doors and effects

- [ ] Every `doors[]` leaf sits at a hinge with a floor under it, and the wall it
      fills authors a matching `kind: door` opening (same offset/width/height).
- [ ] Each door's id is unique across props, lights, doors, volumes, timers and
      spawn points; every binding's target resolves and the capability matches.
- [ ] `open_direction`, `swing_degrees` and `obstruction` match the room: the leaf
      opens into the intended side and never rakes a wall or a static prop.
- [ ] A manual door carries an `interactable` component plus its own `interact`
      binding; an externally controlled door carries neither and is driven only
      by other entities' actions.
- [ ] All glazing that should block the player authors `"solid": true`; a
      decorative pane that should stay walk-through is deliberate.
- [ ] Switchable fixtures (`switchable: true`) are only the ones a map action
      drives; every other fixture leaves the flag false.
- [ ] `effects[]` emitters sit in the space they belong to, use a sensible
      `count`/`size`/`lifetime_seconds`, and are not expected to block or light
      anything.

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
- [ ] `--check-geometry` reports no confirmed defects; a confirmed
      `wall-joint-step` is repaired with `--repair-geometry` plus
      `tools/levels/repair_alignment.py --apply` (with the coupled
      `coupled-review` items resolved deliberately), and every remaining
      heuristic warning is either repaired or covered by a narrow
      `geometry_intent` annotation with a note (§31).
- [ ] A capture (`PLACES_CAPTURE`) has been inspected if practical, at High and
      at `PLACES_QUALITY=low` (and/or `medium`) if reflections or material
      response matter.

---

## 29. Entities, Components, Bindings, Volumes, Timers, Sequences and Spawns

The runtime gives every placed record a stable identity, optional typed
**components** that say what it can do, and **event bindings** that say what
its events do. One closed action set is dispatched by that single mechanism:
props, doors, fixtures, volumes, timers, spawn points and spawn templates all
wire behaviour the same way, and none of it is a scripting engine. This
section is the field-by-field contract; every name below is verified against
`src/level.rs` (`ComponentDef`, `EventBindingDef`, `ConditionDef`, `ActionDef`,
`EventKindName`, `TriggerVolumeDef`) and `src/entities/`
(`TimerDef`, `SequenceDef`, `Spawn*Def`).

> **Format note.** This is the v3 contract and the only current level format.
> The converter's input is the previous schema: a source that still carries the
> previous keys either converts with `python3 tools/levels/convert_v3.py --all`
> or fails to parse its behaviour. A door's own press is an `interact` binding
> with `{ "action": "toggle" }`.

### Identity and namespaces

One **instance namespace** covers every addressable placed record: props,
doors, ceiling fixtures, trigger volumes, timers and spawn points. An authored
`id` wins; deterministic defaults are `<model short name>_<n>` for a prop,
`<fixture short name>_<n>` for a fixture and `trigger_<n>` (1-based) for a
volume, while a door, timer and spawn point always author their id. Ids must be
well-formed (`[A-Za-z0-9:._-]`, no leading `:`), unique in that namespace, and
are what a binding's `target`/`point`/`timer` names resolve against.

**Sequences, spawn templates and spawn groups are separate resource
namespaces**: they are addressed by resource name (`sequence`, `template`,
`group`), so the same string may appear once in each kind. Duplicates within a
kind are errors.

Two copies of one model are two instances with independent state; never key
external state on `model`.

### Components

A component is a reusable capability record. An entity that carries none is
scenery: it cannot be aimed at, switched, animated or read by a condition.
Components never carry actions — the bindings do that.

```json
"components": [
  { "component": "interactable", "prompt": "Sauna switch", "reach": 1.6 },
  { "component": "animation", "clip": "toggle", "looped": false, "playing": false },
  { "component": "state", "name": "phase", "value": "cold" }
]
```

| `component` | Fields (defaults) | What it grants |
| --- | --- | --- |
| `interactable` | `prompt?` (default `"Interact"`), `reach?` (default 2.5 m, `0 < reach ≤ 4.0`), `enabled` (`true`), `label?` | The entity is aimable with **E**; a press emits `interact`. `enabled: false` starts it aimed-but-silent until an `enable` action. `label` is the floating text a `toggle_label` may show. |
| `animation` | `clip` (required, non-blank), `speed` (`1.0`), `looped` (`false`), `playing` (`false`) | The entity owns an animation state: `play_animation`, `toggle_animation` and `animation_complete` need it. `playing` starts the clip at load; the default rests on the authored pose. |
| `audio` | `sound` (required), `gain` (`1.0`), `looped` (`false`), `enabled` (`true`), `playing` (`false`) | Typed emitter state: `play_sound`/`stop_sound` set `playing`/`looped`, `enable`/`disable` gate it, and the frame loop reports a requested sound once. **No audio device backend exists in this tree**, so nothing is audible; no shipped map authors one. |
| `light` | `enabled` (`true`), `switchable` (`false`), `emission_scale` (`1.0`) | A prop's own static light state. **`switchable: true` is rejected on anything but a ceiling fixture**: only a fixture has prepared switchable lightmap layers, and a non-switchable light cannot be a `set_light`/`toggle` target either. |
| `material` | `variants` (≥ 1 of `{ name, emission_scale }`), `current?` (default the first) | The entity selects one of several emission profiles at runtime with `change_material`. Only the emission profile can change; surface materials of baked geometry cannot. |
| `state` | `name` (required), `value` (required: bool, integer, float or string) | A typed state slot a `set_state` may write and a `state` condition may read. Timers and volumes own their states implicitly. |
| `lifetime` | `seconds` (`> 0`) | The entity removes itself after that many simulation seconds. |
| `steam` | `enabled` (`true`) | The entity is a presentation-only steam emitter. |
| `water` | `enabled` (`true`) | The entity is a water volume controller. |
| `nav_agent` | `radius`, `speed_mps`, `height` (`1.8`), `step_height` (`0.4`), `max_slope` (`2.6667`) | The entity's navigable body. Each distinct body becomes one baked agent class; navigation queries select the class that matches it. See §33. |
| `nav_obstacle` | `size?` (`[w,h,d]`, default the resolved size), `affects_nav` (`true`) | An explicit box the offline navigation bake treats as an obstacle, so a proxy the collision build does not carry can still block navigation. |
| `ai` | `behavior` (`idler`), `role?`, `reacts_to` (`[]`), speeds, ranges and catch fields | The entity runs the shared AI behavior (`idler`/`wanderer`/`prey`/`predator`/`follower`). Requires a `nav_agent` body; an entity with a route must not also author `ai`. See §33. |
| `fade` | `period_seconds` (finite, `0 < p ≤ 3600`; default `6.0`, ignored in proximity mode), `phase?` (`0..=1`; omitted = deterministic per-instance), `min_opacity` (`0.0`), `max_opacity` (`1.0`, `≥ min_opacity`), `enabled` (`true`), `near_radius?`/`far_radius?` (finite, `0 < near < far`; both together), `fade_out_seconds?`/`fade_in_seconds?` (finite, `0 < s ≤ 600`; defaults `1.5`/`3.0`, only with the radii) | The entity's opacity cycles on a loop, or — when both radii are authored — fades out as the player approaches and back in as the player retreats. `enabled: false` holds `max_opacity`. A fade-only entity still fades without a route, AI or binding. |
| `glow` | `color?` (`[1.0, 0.86, 0.6]`, each channel `0..=1`), `intensity?` (`0.5`, `0..=8`), `range?` (`3.0` m, `0.05..=64`), `socket?` (animated joint/node name, non-blank), `offset?` (`[0, 0, 0]` entity-local metres, each `|v| ≤ 4`; used only when no socket resolves), `fade?` (`true`: intensity multiplies the fade opacity) | One attached dynamic light. With a `socket` it follows the animated joint; without one it sits at the entity-local `offset`. |

A component the engine cannot honour is a named load error. Unknown component
tags are rejected at parse time.

#### Fades and attached glows

`fade` and `glow` are presentation components: they change how an entity draws
and how it lights its surroundings, never where it moves. An entity that also
authors a `routes[]` entry or an `ai` component still walks its route or
wanders exactly as before; the fade and the attached light ride along. A placed
entity carrying either component (or both) gets a renderer frame even with no
route, AI, animation override or binding, so a fade-only ghost fades in place.

```json
{ "component": "fade", "period_seconds": 6.0, "phase": 0.25,
  "min_opacity": 0.0, "max_opacity": 1.0, "enabled": true }
```

An omitted `phase` resolves a deterministic per-instance value from the
instance id (FNV-1a over its bytes), so two copies of one model desynchronise
without an authored phase and reloading a level never re-rolls the cycle.
A disabled fade holds `max_opacity` at every instant.

**Proximity fades.** When both `near_radius` and `far_radius` are authored, the
opacity is driven by the player's **horizontal distance to the entity's live
position** — the routed or AI position when it moves, its placement when it
does not — instead of by the clock. `period_seconds` and `phase` are ignored in
this mode; `min_opacity` (default `0.0`, fully hidden) and `max_opacity` are the
ends of the range.

```json
{ "component": "fade", "near_radius": 3.0, "far_radius": 6.0,
  "fade_out_seconds": 1.5, "fade_in_seconds": 3.0 }
```

* **Hysteresis.** The entity begins fading **out** once the player is strictly
  inside `near_radius` and begins fading **in** only once the player is strictly
  beyond `far_radius`. Between the two radii the current direction holds, so
  walking across the band never flaps. A hidden ghost stays hidden while the
  player loiters in the band and returns only when they retreat past
  `far_radius`.
* **Frame-rate independent, no jumps.** The live opacity advances from its
  *current* value at `1 / fade_out_seconds` (or `1 / fade_in_seconds`) of the
  full `min_opacity..=max_opacity` range per second, integrated in the same
  fixed `1/120 s` substep the vertical motion uses. A fade interrupted mid-way
  reverses from where it is: nothing snaps to an endpoint, and 30, 60 and
  144 fps produce the same opacity for the same elapsed time.
* **The controller is live state, not a spawn.** The entity is never removed,
  duplicated or re-instantiated: a fully hidden ghost keeps following its route
  or AI (and its frame keeps being published, at opacity `0`), and the same
  instance fades back in. `enabled: false` pins it to `max_opacity` until an
  `enable` action resumes the controller.
* **Coherent visuals.** The frame opacity drives the surface alpha *and* the
  `glow.fade` intensity, so a hidden entity's attached light goes out with it.
  A fully hidden entity draws in neither the opaque nor the translucent pass,
  so it leaves no depth or shadow imprint. A fadeable model must export a
  blended material (`alphaMode: "BLEND"`, as the shipped `sheet-ghost` and
  `sheet-ghost-cat` do); an opaque character cannot express partial opacity.

**Scale-appropriate values.** The hysteresis band should sit just outside the
entity's visible body, and the out time should read as "it notices you" rather
than "it teleported":

| Entity | `near_radius` | `far_radius` | `fade_out_seconds` | `fade_in_seconds` |
| --- | --- | --- | --- | --- |
| ~1.6 m sheet ghost | `3.0` m | `6.0` m | `1.5` s | `3.0` s |
| ~0.35 m ghost cat | `1.0` m | `2.2` m | `0.8` s | `1.6` s |

The cat's band is tighter and quicker because its whole body is a metre smaller;
the ghost's slower fade-in lets it re-form gradually as the player walks away.

```json
{ "component": "glow", "color": [0.45, 0.95, 1.0], "intensity": 0.6,
  "range": 3.5, "socket": "flame", "offset": [0.0, 0.1, 0.0], "fade": true }
```

* `socket` names a node of the animated model (matched case-insensitively) and
  the light rides that joint's animated transform; **the offset is only a
  fallback**: it is used when no socket is authored *or* the named node does not
  resolve, and an authored `offset` is ignored whenever the socket does
  resolve. The offset is in entity-local metres.
* `fade: true` multiplies the intensity by the entity's live fade opacity, so
  an attached light dims with its ghost; `false` keeps it constant.
* A `glow` may be authored on a plain prop, but in this build only a skinned
  character driven through the entity frame updates it at runtime.
* At most one `fade` and one `glow` per entity, like every other component
  kind except `state`. A non-finite or out-of-range value is a named load
  error, never a clamp: the loader, the runtime and `tools/assets/validate.py`
  agree on the ranges in the table above.

#### Note for the Halloween entity fixture (Job 04)

The Halloween archetypes are authored by these ids; a `glow` names the animated
joint below (matched case-insensitively) and the light rides that joint's
animated transform through the jump, the float and the walk:

| Archetype id | `glow.socket` | Clips the GLB carries | Reference speed |
| --- | --- | --- | --- |
| `carved-pumpkin` | `flame` | `laugh`, `hop_forward` | `hop_forward` 0.5 m/s |
| `sheet-ghost` | `body` | `float_forward`, `idle` | `float_forward` 0.3 m/s |
| `pumpkin-skeleton` | `piece_pumpkin_head` | `walk`, `collapse_reassemble` | `walk` 0.571 m/s |

The GLBs carry these clips with their `asset.extras.places_entity_clips`
metadata. `fade` and `glow` never change route or AI movement: an entity still
follows its `routes[]` steps or its AI behavior exactly as authored, and its
clips resolve through the renderer's existing cue path. A model without a
literal `walk` clip plays its declared locomotion kind on a `move_to` step or
an AI walk state (the pumpkin's `hop_forward`, the ghost's `float_forward`).

The tested example definitions live in
`tests/fixtures/levels/halloween_entities.json`: the pumpkin hops a walkway
with `laugh` at both reversal points, three ghosts float bounded routes with
independent fade phases (0.0 / 0.35 / authored-default), and the pumpkin-head
skeleton wanders the tree zone through `nav_agent` + `ai`. A jump pumpkin and
a fading ghost:

```json
{ "id": "pumpkin_hopper", "model": "carved-pumpkin",
  "x": 0.0, "z": -2.5, "size": [0.701, 0.654, 0.635], "solid": false,
  "components": [
    { "component": "glow", "color": [1.0, 0.52, 0.16], "intensity": 0.7,
      "range": 4.5, "socket": "flame", "fade": false } ] }
```
```json
{ "id": "ghost_a", "model": "sheet-ghost", "x": 5.0, "z": -8.0,
  "size": [0.994, 1.619, 0.716], "solid": false,
  "components": [
    { "component": "fade", "period_seconds": 7.0, "phase": 0.0,
      "min_opacity": 0.05, "max_opacity": 0.85 },
    { "component": "glow", "color": [0.4, 0.95, 1.0], "intensity": 0.45,
      "range": 3.0, "socket": "body", "fade": true } ] }
```

Each entity is `solid: false` and gets its motion from a `routes[]` step (the
pumpkin and ghosts) or `ai` (the skeleton); the ghost's hover is its own bind
pose, so it is placed at `y = 0` and still floats. Give a moving entity a
`glow`, never a static `props[].lights` entry: a baked light at its spawn would
contradict the runtime motion.

### Event bindings

A binding lives on the entity that **emits** the event, so wiring is local: a
switch's press lists what the press does, a volume's entry lists what entering
does. There is exactly one binding mechanism.

```json
"bindings": [
  { "id": null,                  // optional; diagnostics and named references.
    "on": "interact",            // REQUIRED; closed event-kind enum.
    "key": null,                 // optional event-key filter (see below).
    "when": [],                  // optional conditions; all must hold to run.
    "once": false,               // run at most once per run; a reset re-arms it.
    "cooldown_seconds": 0.0,     // seconds before it may run again; >= 0.
    "actions": [ { "action": "toggle_label" } ] }  // REQUIRED, 1..8, in order.
]
```

At most 16 bindings per entity. `key` narrows an event that carries one: a
timer fires with its own id, a sequence with its id, an animation with its
clip. `once` and `cooldown_seconds` are per binding, not per action.

**Event kinds.** A binding must listen for an event its own entity can emit;
the loader rejects a `timer` binding on a non-timer, an `interact` binding on
an entity with no enabled `interactable`, and so on.

| `on` | Fires when | Carrier |
| --- | --- | --- |
| `interact` | the player presses E on the entity (edge-latched, never auto-repeat) | any record with an enabled `interactable` component |
| `enter_volume` | the player's feet enter the volume's band, or a fast fall sweeps through it | a `volumes[]` entry |
| `exit_volume` | the player's feet leave the band | a `volumes[]` entry |
| `timer` | a timer reaches zero | a `timers[]` entry (`key` = its id) |
| `object_state` | one of the entity's states changes value | any entity with a `state` component |
| `sequence_complete` | a sequence running on the entity completes (`key` = sequence id) | any entity |
| `spawn` | the entity is spawned | a spawn template's instances |
| `animation_complete` | a played clip reaches its end | an entity with an `animation` component |
| `ai_state`, `caught` | reserved for the navigation/AI upgrade; they never fire today | — |

**Conditions.** A binding with a `when` list runs only if every condition
holds. A condition whose target does not resolve is a load error.

| `check` | Fields | True when |
| --- | --- | --- |
| `state` | `target`, `name`, `equals` | the target's state matches `equals` exactly (same type and value) |
| `enabled` / `disabled` | `target` | the target's capability state is on / off |
| `locked` / `unlocked` | `target` | the target door is locked / unlocked |
| `door_open` / `door_closed` | `target` | the target door is at its open / closed end |
| `sequence_running` / `sequence_idle` | `target` | a sequence is running / not running on the target |

### Actions

One binding or sequence step runs 1..8 actions in order; at most one action
batch starts per frame, and `reset_to_start` ends its batch (later actions do
not run). Every action is typed; an unknown tag is a parse error, an action
whose target cannot do the thing is a named load error.

| Action | Fields | Legal target | Effect |
| --- | --- | --- | --- |
| `open` / `close` | `target?` (omitted = the actor) | a door | drive the leaf toward its open / closed end |
| `toggle` | `target?` | a door **or** a switchable ceiling fixture (`ceiling_lights` with `"switchable": true`) | flip the door between its ends mid-travel, or flip the fixture's switch |
| `set_light` | `target?`, `on` (required bool) | a switchable ceiling fixture | set the fixture explicitly on/off; the prepared lightmap layer follows |
| `lock` / `unlock` | `target?` | a door | refuse/resume opening until unlocked |
| `enable` / `disable` | `target?` | any entity | aiming, emission, animation and audio capability on/off (a disabled interactable emits no `interact`) |
| `play_animation` | `target?`, `clip?` (non-blank when present), `loop?` | an entity with an `animation` component | play a clip; a one-shot holds its last pose, `loop: true` repeats |
| `toggle_animation` | `target?`, `clip?` | an entity with an `animation` component | scrub the clip toward the opposite end of its timeline (a lever flip); a second press mid-move reverses |
| `play_sound` / `stop_sound` | `target?`, `sound?`, `loop?` | an entity with an `audio` component | sets the emitter's `playing`/`looped` state and emits a playback command; this build has no audio device backend, so the command is reported once and nothing plays |
| `change_material` | `target?`, `variant` (required) | a runtime instance of a spawn template with a `material` component (a baked static prop is rejected: its material is prepared geometry) | select a declared emission variant |
| `move_object` | `target?`, `x`, `z` (required finite), `y?`, `speed?` | a runtime instance of a spawn template (a baked static prop or a door is rejected: drive a door with `open`/`close`/`toggle`) | move a runtime entity collision-respecting |
| `steam` component + `enable`/`disable` | `target?` | an entity with a `steam` component | turns the authored emitter's billboards on or off through the renderer (a disabled emitter's level effect stays, it just stops drawing) |
| water volume + `enable`/`disable` | `target?` | a water-volume entity (`water_<n>`) | removes the volume from the controller's water sampling: the baked surface still draws, the player walks or falls through |
| `set_state` | `target?`, `name` (required), `value` (required bool/int/float/string) | the target must already author that state, or be a timer/volume (the runtime owns those states) | write a typed state |
| `toggle_weather` | — | — | immediately alternate the authored `weather` / `weather_alternate` configurations for this session; both are required; cancels a cycle |
| `set_weather_strength` | `strength` | `transition_seconds: 0` | smoothly interpolate compatible weather endpoints at strength 0–1 over 0–300 seconds; cancels a cycle |
| `set_weather_cycle` | `enabled` | — | start or stop the authored `weather_cycle`; stopping holds the current strength unless paired with a manual action |
| `set_prompt` | `target?`, `prompt` (nonblank) | an entity with an `interactable` component | replace the aimed-at interaction prompt; reset/reload restores the authored prompt |
| `toggle_label` | `target?` | a placed **prop** (only a placed prop shows a label) | show/hide the instance's floating display name |
| `start_sequence` | `sequence` (required, known id), `target?` (omitted = the actor) | the entity the sequence should run on | start/replace a sequence on the target |
| `stop_sequence` | `target?` | any entity | stop the sequence running on the target |
| `start_timer` | `target?`, `seconds?` (positive override), `repeat?` | a timer | arm the timer (a running timer restarts from a full period) |
| `stop_timer` | `target?` | a timer | stop counting without changing its period |
| `spawn_entity` | `template?`, `point?`, `group?`, `name?` | — (see below) | instantiate a template at a point |
| `despawn_entity` | `target` (required) | an entity id, a spawn-template id or a spawn-group id | remove a live instance (or a whole group's member) |
| `reset_to_start` | — | — | return the player to the authored spawn, re-arm triggers, clear pose/route overrides and return doors to `initial_state`; ends the batch |

`spawn_entity` resolution: with a `point`, the point's own template is used
unless `template` overrides it; a `template` alone has no world position and is
rejected; with neither field the acting entity must itself be a spawn point.
`group` overrides the point's group, and `name` gives the instance a runtime
name later actions can address (default `<point>#<n>`).

### Object interactions (E)

E (interact) and C (crouch) can be rebound in Settings > Controls and persist
in `settings.json`. C toggles a 0.9 m body with a 0.8 m eye height. Standing is
blocked while the standing body would overlap an overhead obstacle.

A record with an enabled `interactable` component is aimable — every prop that
authors one, and every such door:

* The player looks at it and presses **E**.
* Targeting uses the actual eye position (a crouched player aims from the
  crouched eye), the component's `reach` (default 2.5 m, maximum 4.0 m) and
  the collision world as occluders: no interaction through a wall or with an
  object hidden behind a nearer solid.
* One press is one interaction: the key edge is latched, so holding E never
  repeats; menus, pause and lost focus do not latch.
* `toggle_label` shows or hides a floating display name above *that placed
  prop instance only* (doors have no label in v3). State is per instance, so
  the other copy of the same model is untouched; reloading a level hides every
  label again, while `reset_to_start` keeps them.
* Labels are drawn with the existing UI text pipeline, projected from the
  object's world anchor, clamped to the viewport and hidden when occlusion
  blocks the line of sight.

The aimable bound is the same contract as collision: the authored `size`
(scaled) or the standard `[0.6, 0.9, 0.6]` prop box — never the catalogue
size. Author `size` on an interactable whose rendered model is much taller or
wider than that box (the demo's `spooner_man` authors `[0.7, 1.8, 0.7]`) so
the crosshair covers the object, not just its feet.

A routed entity's label anchor and aim bound follow it: the controller
republishes them from the authored rest values plus the live offset every
frame, so E and the floating name track a character that walks away.

### Trigger volumes

A volume is an axis-aligned box: a rectangular `(x, z)` footprint plus a
vertical `bottom_y..top_y` band. The controller tests the player's feet
against it every frame and emits `enter_volume` / `exit_volume` on the edges;
the volume's own bindings decide what those events do.

```jsonc
{
  "id": "pit_hole_1",              // optional; default trigger_<n>
  "x": 9.6, "z": -26.2,            // optional; footprint MIN corner, default 0
  "width": 1.6, "depth": 1.6,      // REQUIRED, > 0
  "bottom_y": -3.2,                // optional; default the walkable floor under the centre
  "top_y": -0.05,                  // optional; default bottom_y + 2.0
  "bindings": [
    { "on": "enter_volume", "once": false,
      "cooldown_seconds": 0.5,     // carried by the binding, not the volume
      "actions": [{ "action": "reset_to_start" }] }
  ]
}
```

Semantics:

* **Enter, not overlap.** The volume fires on the first frame the player's
  feet are inside the band, or when the frame's swept feet segment crosses it,
  so a fast fall through a thin band still counts. Standing inside never
  re-fires; leaving re-arms.
* `cooldown_seconds` on the `enter_volume` binding bounds a re-entry; `once`
  makes that binding fire at most once per run (a `reset_to_start` re-arms it).
* `reset_to_start` returns the player to the authored spawn and facing, clears
  velocity, stance, ladder and water state, and re-seeds every trigger from
  the new position, so a teleport never activates the volumes between the old
  and new positions and a spawn inside a volume never loops.
* At most one volume batch starts per frame; if the frame crosses more than
  one volume, the later entries are deferred to following frames.
* The footprint must overlap a room; the resolved `top_y` must be above
  `bottom_y`. Both are named load errors.

Level 0: The Pit authors one `reset_to_start` volume inside each of the 15
carpet holes (the recessed 1.6×1.6 `floor_regions` at `offset_y: -3.2`); the
`top_y` sits 5 cm below the hall floor so standing on the carpet beside a hole
never counts as an entry, and the bottom reaches the hole floor so any fall
crosses the band:

```json
{ "id": "pit_hole_1", "x": 9.6, "z": -26.2, "width": 1.6, "depth": 1.6,
  "bottom_y": -3.2, "top_y": -0.05,
  "bindings": [ { "on": "enter_volume", "cooldown_seconds": 0.5,
                   "actions": [{ "action": "reset_to_start" }] } ] }
```

### Timers

A timer is a **headless entity**: it has an id, a period and bindings, but no
geometry and no world position. It fires `on: "timer"` bindings when it
reaches zero; the event key is the timer's id.

```json
"timers": [
  { "id": "sauna_warmup_timer", "seconds": 1.5, "repeat": false, "autostart": false,
    "bindings": [
      { "on": "timer",
        "actions": [ { "action": "start_sequence", "sequence": "sauna_warmup",
                       "target": "sauna_door" } ] }
    ] }
]
```

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | string | **yes** | — | Instance id; unique in the instance namespace. |
| `seconds` | number | **yes** | — | Finite, `> 0`. |
| `repeat` | boolean | no | `false` | Re-arm after each fire instead of stopping. A long frame never burst-fires a repeating timer: the next fire is a full period later. |
| `autostart` | boolean | no | `false` | Start counting at load (and on every reset) instead of waiting for `start_timer`. |
| `bindings` | array | no | `[]` | Usually one `on: "timer"` binding. |

`start_timer` may override the period and repeat flag for the current arm;
`stop_timer` halts it; `reset_to_start` restores the authored values.

### Sequences

A sequence is an ordered list of steps the runtime executes **on one entity**.
Starting a second sequence on an entity replaces the first; despawning the
owner cancels it; completion emits `sequence_complete` with the sequence id as
the key.

```json
"sequences": [
  { "id": "sauna_warmup", "looped": false, "steps": [
      { "step": "wait", "seconds": 0.5 },
      { "step": "set_state", "name": "phase", "value": "warm" },
      { "step": "emit", "on": "timer", "key": "warm" }
    ] }
]
```

| Step | Fields | Notes |
| --- | --- | --- |
| `action` | `action` (one ordinary action) | runs one action, then advances |
| `wait` | `seconds` | simulation seconds, `0..600` |
| `move` | `x`, `z`, `speed` (finite, `> 0`), `y?` | walks collision-respecting to a world position |
| `face` | `yaw_degrees` | turns at the entity's turn rate |
| `wait_animation` | `clip?`, `timeout` | waits for a clip (or any playing clip) with a `0..120` s timeout |
| `emit` | `on` (event kind), `key?` | emits an event from the sequence's entity |
| `set_state` | `name`, `value` | writes a state the owner already authors (or a timer/volume state) |
| `stop` | — | ends the sequence here, as if it completed |

A sequence runs only on entities an authored `start_sequence` names (directly
or through another sequence). A step with an omitted target acts on that
owner; the loader rejects a sequence that is never started on any entity.
Zero-delay cycles (a chain that could recurse without consuming a
wait/timer/animation) are rejected at load.

### Spawn templates, points and groups

Spawning is authored, never scripted: a template is a typed prefab, a point
says where it appears, and a group enforces the encounter rule.

```json
"spawn_templates": [
  { "id": "crate_spawn", "model": "core:crate", "scale": 0.5,
    "lifetime_seconds": 20.0,
    "components": [ { "component": "state", "name": "phase", "value": "spawned" } ],
    "bindings": [] }
],
"spawn_points": [
  { "id": "crate_spawn_point", "x": 12.0, "z": 16.3, "yaw_degrees": 0.0,
    "template": "crate_spawn", "group": "crate_group", "bindings": [] }
],
"spawn_groups": [ { "id": "crate_group", "at_most_one_active": true } ]
```

| Record | Fields (defaults) |
| --- | --- |
| `spawn_templates[]` | `id` (required), `model` (required), `scale` (`1.0`), `lifetime_seconds?` (despawn after that many seconds; omitted lives until removed), `components` (`[]`), `bindings` (`[]`) |
| `spawn_points[]` | `id` (required), `x`/`z` (default `0.0`), `y?` (default the walkable floor under the point), `yaw_degrees` (`0.0`), `template` (required, known id), `group?` (known id), `bindings` (`[]`) |
| `spawn_groups[]` | `id` (required), `at_most_one_active` (`false`) |

* The template's `components` are the components every instance is **born
  with**; its `bindings` are the bindings a `spawn` event can fire on the new
  instance.
* A group with `at_most_one_active: true` refuses a second spawn while one
  member is alive — the same group may be shared by several spawn points, which
  is the encounter mechanism (several candidate spots, one live member). The
  member's own despawn (lifetime expiry, `despawn_entity`, or a reset) releases
  the group.
* Spawn visibility is immediate: the runtime entity exists the same tick, and
  its render command is drained before the frame draws. Despawn removes
  collision and interaction participation immediately.

### Entity routes

A `routes[]` entry drives **one placed skinned entity** (a prop whose catalogue
model carries a glTF skin) through a short authored sequence. Routes are keyed by
the placed instance id, so two copies of a model run independently:

```jsonc
{
  "id": "rat_1",                 // REQUIRED: a placed prop instance id
  "loop": true,                  // optional, default false: restart after the last step
  "steps": [                     // REQUIRED, 1..64
    { "step": "move_to", "x": 18.0, "z": 6.0, "speed": 0.35 },
    { "step": "move_to", "x": 22.0, "z": 9.0, "speed": 1.2 },
    { "step": "face", "yaw_degrees": 90.0 },
    { "step": "wait", "seconds": 1.0 },
    { "step": "play", "clip": "idle", "seconds": 2.0, "loop": true }
  ]
}
```

* `move_to` walks in a straight line to a waypoint at `speed` m/s (0 < speed <= 6).
  The runtime follows the walkable floor, refuses a step taller than 0.3 m and
  stalls (reported once in the console) if a wall, a drop or the void blocks it —
  it never tunnels or teleports. Validation samples the whole straight segment at
  the entity's own footprint and refuses a waypoint off the floor or a path
  through geometry.
* `face` turns in place at 240 deg/s; `wait` stands still.
* `play` plays a named clip for `seconds`, then advances. The GLB's own clip
  metadata (`asset.extras.places_entity_clips`) records each clip's loop flag and
  the ground speed its stride is authored for; `move_to` uses it so the walk/run
  cycle matches the route speed with no foot sliding.
* The entity must be **non-solid**: a `solid: true` prop's own box would block its
  first step and is a named validation error.
* A route `id` must name a placed prop and be unique among routes. Waypoints must
  sit on a real walkable surface of the level.

Entity sizes, forward axis and stride speeds come from the asset's own README
(see `assets/entities/<id>/README.md`); the fixtures under
`tests/fixtures/levels/` demonstrate walk, run, pose selection and two
independent instances.

### Places Demo maintenance notes

The Home loop uses four contiguous narrow room footprints, with 1.4 m clear
corridors, 2.5 m ceilings and sparse warm fixtures. Only the three internal
room junctions carry `missing-wall` intent; the exterior remains enclosed.
The kitchen knee walls and posts use joined `walls` so the wall union removes
internal faces, with all four pieces aligned to a 0.26 m module. The pool
passage west jamb meets the stair-hall partition face at x = 19.15 m.
The curtain-side wet area uses narrow adjoining floor patches to form an
irregular connected outline. All retain `core:pool_deck_wet_01`, the existing
world-aligned tile artwork, sheen and live planar reflection.
The cat starts walking immediately at 0.32 m/s and retains its six seated
stops across the level. The existing reflection tests continue to exercise the shipped wet surface.

### Places Demo examples

The shipped demo authors real v3 entities; use them as the reference (all in
`assets/levels/places_demo.json`):

```json
{ "id": "water_cooler", "display_name": "Water Cooler", "model": "core:water_cooler",
  "x": 0.6, "y": 0.0, "z": 0.6, "rotation_degrees": 90.0,
  "size": [0.35, 1.1, 0.35], "solid": true,
  "components": [ { "component": "interactable", "prompt": "Toggle name" } ],
  "bindings": [ { "on": "interact",
                  "actions": [{ "action": "toggle_label" }] } ] }
```

```json
{ "id": "spooner_man", "display_name": "Spooner-Man", "model": "spooner-man",
  "x": 5.6, "y": 0.0, "z": 8.4, "rotation_degrees": 90.0,
  "size": [0.7, 1.8, 0.7],
  "components": [ { "component": "interactable", "prompt": "Toggle name" } ],
  "bindings": [ { "on": "interact",
                  "actions": [{ "action": "toggle_label" }] } ] }
```

A wall switch composes the lever animation with the instance-local label in
one binding — the switch does not lose either behaviour, and the `animation`
component is what lets `toggle_animation` act on it:

```json
{ "id": "kitchen_switch", "display_name": "Kitchen Light Switch",
  "model": "home:wall_switch", "x": 58.3, "y": 1.2, "z": 8.47,
  "rotation_degrees": 270.0, "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Switch", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [
    { "on": "interact",
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "toggle_label" }] }
  ] }
```

`reset_to_start` returns every toggle to its rest end (`t = 0`) rather than
dropping it, so a reset switch does not keep the pose its last press left it
in.

### Complete example: a switch that controls a light

The demo's `sauna_switch` flips its switchable fixture in one press. `toggle`
on a light flips it between on and off; `set_light` with an explicit `"on"` is
the deterministic alternative. The lever's animation works because the prop
itself carries the `animation` component:

```json
{ "id": "sauna_switch", "display_name": "Sauna Switch", "model": "home:wall_switch",
  "x": 26.16, "y": 1.2, "z": 14.3, "rotation_degrees": 90.0,
  "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Sauna switch", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [
    { "on": "interact",
      "actions": [
        { "action": "toggle_animation", "clip": "toggle" },
        { "action": "toggle", "target": "sauna_light" }
      ] }
  ] }
```

A press activates the switch and the one target it names: the sauna leaf keeps
its own `interact` binding, so the switch must not also name the door, or one
press would move both (see
[Multi-action switch](#multi-action-switch-lever--light-or-lever--door)).

A **second, independent switch** drives presentation-only steam through two
mutually exclusive branches, selected by the switch's own `state` value. Each
branch checks the state it is about to leave; exactly one `when` clause matches
on any press, so one press toggles exactly the steam and never the lamp or a
door:

```json
{ "id": "sauna_steam_switch", "display_name": "Sauna Steam Switch",
  "model": "home:wall_switch", "x": 27.6, "y": 1.2, "z": 14.8,
  "rotation_degrees": 180.0, "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Sauna steam", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false },
    { "component": "state", "name": "steam", "value": "off" }
  ],
  "bindings": [
    { "id": "steam_off", "on": "interact",
      "when": [{ "check": "state", "target": "sauna_steam_switch",
                 "name": "steam", "equals": "on" }],
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "disable", "target": "sauna_steam_a" },
                  { "action": "disable", "target": "sauna_steam_b" },
                  { "action": "set_state", "name": "steam", "value": "off" }] },
    { "id": "steam_on", "on": "interact",
      "when": [{ "check": "state", "target": "sauna_steam_switch",
                 "name": "steam", "equals": "off" }],
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "enable", "target": "sauna_steam_a" },
                  { "action": "enable", "target": "sauna_steam_b" },
                  { "action": "set_state", "name": "steam", "value": "on" }] }
  ] }
```

A switch's interaction bound must protrude from the wall face it is mounted on
(back plane on or in front of the face), or the wall itself can occlude the
aimed ray at oblique or upward angles: the interaction target is the point the
ray first enters the instance's box, so a bound buried in the wall lets the wall
win. `home:wall_switch` instances sit `size` deep with their back plane on the
face.

```json
{ "fixture": "core:fluorescent_panel_01", "id": "sauna_light",
  "x": 31.0, "z": 13.0, "brightness": 0.9, "switchable": true }
```

The light toggles through its **prepared lightmap layer**: the switch selects
the on or off lighting the compiler baked, so the room's illumination really
changes; a fixture that is not `switchable` never changes illumination, and an
`animated_emissions[]` flicker animates only the face's glow.

### Complete example: trigger volume → timer → sequence

One small volume, one headless timer and one sequence chain a state change and
an emitted cue to the sauna threshold. The volume arms the timer once; the
timer starts the sequence on the door; the sequence waits, writes the door's
state and emits a keyed event:

```json
"volumes": [
  { "id": "sauna_warmup_zone", "x": 26.5, "z": 12.7, "width": 1.2, "depth": 1.2,
    "bindings": [
      { "on": "enter_volume", "once": true,
        "actions": [
          { "action": "set_state", "target": "sauna_warmup_timer",
            "name": "phase", "value": "armed" },
          { "action": "start_timer", "target": "sauna_warmup_timer" }
        ] }
    ] }
],
"timers": [
  { "id": "sauna_warmup_timer", "seconds": 1.5, "repeat": false,
    "bindings": [
      { "on": "timer",
        "actions": [ { "action": "start_sequence", "sequence": "sauna_warmup",
                       "target": "sauna_door" } ] }
    ] }
],
"sequences": [
  { "id": "sauna_warmup", "steps": [
      { "step": "wait", "seconds": 0.5 },
      { "step": "set_state", "name": "phase", "value": "warm" },
      { "step": "emit", "on": "timer", "key": "warm" }
    ] }
]
```

The door must author the state the sequence writes, so `sauna_door` carries
`{ "component": "state", "name": "phase", "value": "cold" }` beside its
interactable component:

```json
{ "id": "sauna_door", "x": 26.08, "y": 0.0, "z": 12.5,
  "rotation_degrees": 270.0, "width": 1.6, "height": 2.1, "thickness": 0.05,
  "open_direction": "left", "swing_degrees": 95.0, "open_speed_degrees": 90.0,
  "initial_state": "open", "kind": "sauna",
  "components": [
    { "component": "interactable", "prompt": "Sauna door" },
    { "component": "state", "name": "phase", "value": "cold" }
  ],
  "bindings": [ { "on": "interact", "actions": [{ "action": "toggle" }] } ] }
```

---
## 30. Doors, Switches and Effects

A door is a single movable leaf with its own state machine, collision and
component/binding surface; `effects[]` adds presentation-only ambient emitters.
Every field below is verified against `src/level.rs` (`DoorDef`, `DoorFrame`,
`LevelDef::door_frame`, `EffectDef`), `src/loader.rs` (`validate_doors`,
`validate_effects`, `validate_bindings`), `src/door.rs` and the build in
`src/render/common/doors.rs`.

### Doors

A door is placed by its **hinge edge**. `(x, y, z)` is the bottom of the hinge
jamb: `x`/`z` are the hinge's world position and `y` is the leaf bottom **above the
walkable floor under the hinge** (like a prop's `y`, not a wall's absolute base).
`rotation_degrees` aims the **closed leaf**: `0` runs toward `+X`, `90` toward
`-Z`. The leaf extends `width` metres from the hinge along that direction, is
`height` metres tall and `thickness` metres thick.

The wall opening the leaf fills is authored **separately on the wall**, exactly
like any other aperture: a walk-through opening (`"kind": "door"`, or
`"passage"` where the map already labels a wide connection) whose `offset`,
`width` and `height` match the leaf. The loader proves the closed leaf does not
start inside solid geometry (hinge, centre and latch edge are sampled) and that
the hinge stands where the walkable floor resolves; a leaf in a wall or floating
over the void is a named error.

The hinge-to-opening relationship limits the swing. A candidate pose is proven
by sweeping the leaf's **centre plane** (samples across the width and up the
height) against the solid world, and a pose with any sample inside solid
geometry is refused. A leaf hinged **on the opening's edge** therefore cannot
pass 90°: past it the leaf's own hinge-side quarter crosses back into the jamb,
so every candidate pose is refused and the leaf never moves from its authored
state. Keep an edge-hinged leaf at `abs(swing_degrees) ≤ 90`. A leaf whose
hinge stands inside a wall slab may exceed 90° only if its sweep leaves the
slab first; the accepted engine range stays `5`–`179`.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `id` | string | **yes** | — | Stable entity id. Must be unique across props, light fixtures, doors, trigger volumes, timers and spawn points; a duplicate or malformed id is a load error. |
| `x`, `z` | number | **yes** | — | World X/Z of the hinge edge. The hinge must sit over a walkable floor. |
| `y` | number | no | `0.0` | Leaf bottom above the walkable floor under the hinge. |
| `rotation_degrees` | number | no | `0.0` | Yaw of the closed leaf: `0` runs toward `+X`, `90` toward `-Z`. |
| `width` | number | **yes** | — | Leaf width from the hinge to the latch edge, `> 0`, `≤ 12` m. |
| `height` | number | **yes** | — | Leaf height, `> 0`, `≤ 12` m. |
| `thickness` | number | no | `0.045` | Leaf thickness, `> 0`, `≤ 12` m. |
| `open_direction` | `"left"` \| `"right"` | no | `"left"` | Which way the leaf swings about the hinge (seen from above with the closed leaf running hinge→latch). `left` is a positive rotation, `right` negative. |
| `swing_degrees` | number | no | `90.0` | Opening angle, `5`–`179`; a negative value means the opposite swing. An edge-hinged leaf is limited to `abs(value) ≤ 90` (see above). |
| `open_speed_degrees` | number | no | `120.0` | Angular speed while opening, in degrees/second, `> 0`, `≤ 720`. |
| `close_speed_degrees` | number | no | `open_speed_degrees` | Angular speed while closing. |
| `initial_state` | `"closed"` \| `"open"` | no | `"closed"` | Start at angle 0 or at the full swing. |
| `locked` | boolean | no | `false` | `true` starts the leaf locked: it refuses to open (and reports the refusal once) until an `unlock` action runs. |
| `components` | array | no | `[]` | Typed components. A manually interactable door authors `{ "component": "interactable", "prompt": "…" }`; a door that only map actions drive carries neither component nor binding. |
| `bindings` | array | no | `[]` | What the leaf's events do. A manual door authors `{ "on": "interact", "actions": [{ "action": "toggle" }] }`. |
| `obstruction` | `"stop"` \| `"reverse"` | no | `"stop"` | What the sweep does when it meets the player or solid geometry. `stop` holds and resumes when clear; `reverse` flips direction once per obstruction (a 0.4 s guard stops chatter). |
| `kind` | `"interior"` \| `"sauna"` | no | `"interior"` | Visual build (see below). |
| `material`, `frame_material`, `handle_material` | string | no | kind defaults | Per-door material overrides for the leaf, the static frame and the handle. |
| `frame_depth` | number | no | resolved wall (`0.12` standalone) | Total depth of the frame's reveal liner along the leaf's closed normal, in metres; `> 0` and `≤ 2.0` (`MAX_DOOR_FRAME_DEPTH_M`). Omitted resolves the wall the leaf is installed in. |
| `frame_center` | number | no | resolved wall (`0.0` standalone) | Signed offset of the liner's centre from the leaf's centre plane along the leaf's closed normal (hinge-space `+Z`), in metres; `abs(value) ≤ frame_depth`, and it may only be authored together with `frame_depth`. |

The **frame** the leaf hangs in is resolved from the map unless the leaf authors
it. The resolver looks for the wall whose footprint contains the closed leaf's
midpoint and whose walk-through opening the leaf fills: the opening's `kind` is
`door` or `passage`, its bottom (`wall.y + opening.sill`) is at or below the
leaf's own base (the floor under the hinge plus `y`), and its span overlaps the
leaf by at least half the smaller of the two. The first such wall in authored
order wins, and its thickness becomes `frame_depth` and its centre's signed
distance from the hinge — measured along the closed leaf's normal — becomes
`frame_center`. Measuring against the leaf's base rather than the wall's is what
lets a raised doorway meeting a raised floor frame its leaf, as the demo's
pool-side `sauna_door` does, while a window or a vent never matches. A leaf that
no such opening claims keeps a standalone 0.12 m liner
(`STANDALONE_DOOR_FRAME_DEPTH_M`), just deep enough for both casings to show.
Author `frame_depth`/`frame_center` when the visible tunnel is built by geometry
the resolver cannot see (a facade doorway panel in front of the wall, for
example).

A **manual** door is an `interactable` component plus an `interact` binding;
its aim bound follows the live collider as the leaf swings, and its prompt is
authored inside the component (`"Open"`/`"Close"` by phase when omitted). A
door with neither is **externally controlled**: it moves only when an action
drives it. In v3 a door has **no floating label**; `toggle_label` targets
placed props only.

Two kinds ship:

* **`interior`** — a white painted leaf with two panels recessed 12 mm per
  face inside real stiles and rails, and a round brass handle on both sides.
  Defaults: `home:door_white_01` leaf, `home:baseboard_white_01` frame,
  `core:metal_brass_01` handle.
* **`sauna`** — cedar stiles and rails around a clear glass panel, with a wooden
  round handle. Defaults: `home:sauna_wood_01` leaf and handle,
  `home:baseboard_wood_01` frame; the panel is
  `core:glass_window_clear_01` and draws in the blended pass.

### Frame and leaf builds

Each door draws two models, both built in code in `src/render/common/doors.rs`:
a **frame** that never moves and the **leaf** that swings. The frame is a reveal
**liner** through the resolved tunnel — its two jambs and its head lap 30 mm
into the wall on every side and reach 4 mm into the opening as the stop lip the
closed leaf sits behind — plus a 70 mm **casing** standing 12 mm proud on
**both** end faces and the hinge knuckles and plates on the hinge axis, all in
`frame_material`. The leaf is a stile-and-rail panel door whose two panels are
recessed 12 mm (the `interior` kind) or wooden stiles around a glass panel (the
`sauna` kind). Both models travel the dynamic-object path, so each face bakes a
face shade into its vertex colour and a recess, a casing edge and the liner's
dark reveal read at walking distance instead of resolving into one flat
silhouette. Each submesh samples its own material slot (leaf, frame, handle and
the sauna glass), so `material`, `frame_material` and `handle_material`
overrides are all visible and the sauna glass draws in the blended pass.

Places Demo ships both kinds and three `sauna`
leaves: `sauna_door` in the pool-deck wall (hinge `26.08, 0, 12.5`, rotation
270, authored open), `sauna_shower_door` in the shower-bay wall (hinge
`31.1, 0, 11.0`, rotation 0, authored closed), and `sauna_hall_door` in the
corridor doorway of wall 17 (hinge `33.0, 0, 12.4`, rotation 270, authored
closed, hinged on the north jamb and swinging into the sauna clear of the
benches), all swinging into the sauna. The shower-side leaf is the pattern for a
second doorway into one room: its wall opening is authored on wall 15 exactly
like any other, and all three leaves share the one door state machine, collider
and action set while keeping independent `interact` bindings.

The night route's two entrances author their frame explicitly. A 0.40 m
`outdoor:house_wall_doorway` facade panel in front of the 0.30 m wall builds the
jamb depth the player actually sees, and that panel is a prop the wall resolver
cannot see, so `night_source_door` and `night_house_door` author
`frame_depth: 0.7` and the centre of the real tunnel they sit in
(`frame_center: -0.2` and `+0.2`).

The leaf's collider follows the same angle the renderer draws, so what stops the
player and what is seen can never disagree. A resting leaf costs nothing; only a
moving leaf is advanced. `reset_to_start` returns every door to its authored
`initial_state`.

### Wiring doors and lights

Doors are driven by the same typed action set as props and volumes (§29). A
manual door's `interact` binding toggles it on a press; any prop's or volume's
bindings may also drive it:

| Action | Effect |
| --- | --- |
| `open` / `close` | Drive the target door to its open / closed end. |
| `toggle` | Flip a door between its ends, mid-travel included; on a switchable ceiling fixture (`"switchable": true`), flip its switch. |
| `set_light` | Set a switchable ceiling fixture explicitly with `"on": true`/`false`. |
| `lock` / `unlock` | Refuse/resume opening until unlocked. |
| `reset_to_start` | Return every door to its authored `initial_state` (and ends the batch). |

A `ceiling_lights` entry with `"switchable": true` is a valid `set_light` or
`toggle` target. Targets are validated at load: a duplicate id, an unknown
target, or an action/target combination that is not supported (`open` on a
light fixture, for example) is a named error. At most 8 actions run per
binding or sequence step.

### Interactive door

The demo's `hall_door` (Places Demo): a white interior leaf the player opens with
`E`, authored as an `interactable` component plus its own `interact` -> `toggle`
binding. Its wall authors a matching walk-through opening (a `passage` in the
demo's own labelling):

```json
{ "id": "hall_door", "x": 60.3, "y": 0.0, "z": 3.0,
  "rotation_degrees": 0.0, "width": 1.4, "height": 2.1, "thickness": 0.045,
  "open_direction": "left", "swing_degrees": 90.0, "open_speed_degrees": 130.0,
  "initial_state": "closed",
  "components": [ { "component": "interactable", "prompt": "Hall door" } ],
  "bindings": [ { "on": "interact", "actions": [{ "action": "toggle" }] } ] }
```

```json
{ "kind": "passage", "offset": 7.3, "width": 1.4, "height": 2.1, "sill": 0.0 }
```

### Multi-action switch (lever + light, or lever + door)

One press can compose several actions in order. The demo's `sauna_switch` plays
the lever clip and flips its switchable lamp in one binding; the corridor's
`hall_switch` plays the same clip and toggles the hall door. Both carry the
`animation` component that `toggle_animation` requires. Compose only the
targets the press is meant to drive: a door with its own `interact` binding
keeps that interaction, so a switch that also named the door would move two
targets on one press.

```json
{ "id": "sauna_switch", "display_name": "Sauna Switch",
  "model": "home:wall_switch", "x": 26.16, "y": 1.2, "z": 12.2,
  "rotation_degrees": 90.0, "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Sauna switch", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [
    { "on": "interact",
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "toggle", "target": "sauna_light" }] }
  ] }
```

```json
{ "fixture": "core:fluorescent_panel_01", "id": "sauna_light",
  "x": 31.0, "z": 13.0, "brightness": 0.9, "switchable": true }
```

```json
{ "id": "hall_switch", "display_name": "Hall Light Switch",
  "model": "home:wall_switch", "x": 60.3, "y": 1.2, "z": 3.2,
  "rotation_degrees": 180.0, "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Hall switch", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [
    { "on": "interact",
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "toggle", "target": "hall_door" }] }
  ] }
```

### Triggered door (externally controlled)

`levels/level0_pit.json` authors the `pit_gate` leaf with **no** interactable
component or binding, so it is never an interaction target, and drives it from
two volumes: one opens it as the player approaches, the other closes it once
they are through.

```json
{ "id": "pit_gate", "x": 17.7, "y": 0.0, "z": -16.0,
  "rotation_degrees": 0.0, "width": 1.6, "height": 2.2, "thickness": 0.045,
  "open_direction": "left", "swing_degrees": 92.0, "open_speed_degrees": 110.0,
  "initial_state": "closed" }
```

```json
{ "id": "pit_gate_approach", "x": 17.0, "z": -14.8, "width": 3.0, "depth": 0.5,
  "bottom_y": 0.0, "top_y": 2.0,
  "bindings": [ { "on": "enter_volume", "cooldown_seconds": 1.0,
                  "actions": [{ "action": "open", "target": "pit_gate" }] } ] }
```

```json
{ "id": "pit_gate_passed", "x": 17.0, "z": -16.6, "width": 3.0, "depth": 0.4,
  "bottom_y": 0.0, "top_y": 2.0,
  "bindings": [ { "on": "enter_volume", "cooldown_seconds": 1.5,
                  "actions": [{ "action": "close", "target": "pit_gate" }] } ] }
```

### Externally controlled door (initially open)

The demo's `study_door` starts open, only a map action moves it, and it authors
neither an interactable component nor a binding, so it is never aimed at:

```json
{ "id": "study_door", "x": 64.85, "y": 0.0, "z": 5.8,
  "rotation_degrees": 90.0, "width": 1.4, "height": 2.1, "thickness": 0.045,
  "open_direction": "right", "swing_degrees": 88.0,
  "initial_state": "open" }
```

### Sauna door

The demo's `sauna_door` authors `"kind": "sauna"`; the cedar/glass build and its
default materials come from the kind, and the leaf still needs its matching wall
opening. It also carries the `phase` state the warmup sequence writes:

```json
{ "id": "sauna_door", "x": 26.08, "y": 0.0, "z": 12.5,
  "rotation_degrees": 270.0, "width": 1.6, "height": 2.1, "thickness": 0.05,
  "open_direction": "left", "swing_degrees": 95.0, "open_speed_degrees": 90.0,
  "initial_state": "open", "kind": "sauna",
  "components": [
    { "component": "interactable", "prompt": "Sauna door" },
    { "component": "state", "name": "phase", "value": "cold" }
  ],
  "bindings": [ { "on": "interact", "actions": [{ "action": "toggle" }] } ] }
```

### Effects: steam

`effects[]` is presentation only: an effect never collides, never occludes and
never contributes light to the bake. The only kind is `steam`, a bounded plume of
drifting translucent billboards. `x`/`z` position the emitter, `y` is its base
above the walkable floor, `width`/`depth` are its footprint, `height` is the
plume's rise, `count` is the particle budget, `size` the billboard size, `drift`
the horizontal wander, `lifetime_seconds` how long one particle takes to cross
the plume, and `material` the billboard's material (omitted means the engine's
steam default, `core:steam_01`). An effect also takes the generic `enabled`
flag and its own `bindings` (for example an `on: "interact"` or timer-driven
`enable`/`disable` pair), though no shipped level needs one yet.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `kind` | string | **yes** | — | Only `steam`; any other kind is a named validation error. |
| `id` | string | no | — | Diagnostics only. |
| `x`, `z` | number | no | `0.0` | World position. Must be finite. |
| `y` | number | no | `0.0` | Emitter base above the walkable floor under `(x, z)`. |
| `width`, `depth` | number | no | `0.8` | Emitter footprint, `> 0`. |
| `height` | number | no | `1.6` | Plume height above the emitter, `> 0`. |
| `count` | integer | no | `24` | Particle budget, `1`–`128`. |
| `size` | number | no | `0.35` | Billboard size in metres, `> 0`. |
| `drift` | number | no | `0.0` | Horizontal wander amplitude, `≥ 0`. |
| `lifetime_seconds` | number | no | `3.0` | Seconds one particle takes to cross the plume, `> 0`, `≤ 60`. |
| `material` | string | no | `core:steam_01` | Material id for the billboards. |
| `enabled` | boolean | no | `true` | `false` starts the emitter off; `enable`/`disable` actions drive it. |
| `bindings` | array | no | `[]` | The emitter's own event bindings. |

Two steam emitters in the demo's sauna:

```json
"effects": [
  { "id": "sauna_steam_a", "kind": "steam", "x": 31.0, "y": 0.0, "z": 12.6,
    "width": 0.9, "depth": 0.6, "height": 1.5,
    "count": 20, "size": 0.34, "drift": 0.16, "lifetime_seconds": 3.2 },
  { "id": "sauna_steam_b", "kind": "steam", "x": 31.0, "y": 0.0, "z": 13.6,
    "width": 0.9, "depth": 0.6, "height": 1.7,
    "count": 24, "size": 0.38, "drift": 0.2, "lifetime_seconds": 3.6 }
]
```

A level may declare at most 64 effects, and each effect at most 128 particles.
Do not expect an effect to hide anything behind it, to block a route or to
brighten a room.

**Enabling and disabling is instant.** An `enable`/`disable` action targeting
an effect entity (or the emitter's own `enabled: false`) flips the renderer's
resolved emitter through the same command path as a door or a light, and a
disabled emitter draws nothing on the very next frame — not a fading-out
remnant, because the particle model is stateless: every particle's position,
size and alpha is a pure function of the animation clock, so there is no
per-particle state to drain and nothing lingers. Re-enabling draws the live
clock's plume immediately.

A switch can drive one emitter from two mutually exclusive bindings, so one
press toggles exactly one target; the conditions are evaluated once per event,
before any of that event's actions run, so the second binding never sees the
first's write:

```json
{ "id": "steam_switch", "model": "home:wall_switch", "x": 26.16, "y": 1.2, "z": 12.2,
  "solid": false,
  "components": [ { "component": "interactable", "prompt": "Steam" } ],
  "bindings": [
    { "on": "interact",
      "when": [ { "check": "enabled", "target": "sauna_steam_a" } ],
      "actions": [ { "action": "disable", "target": "sauna_steam_a" } ] },
    { "on": "interact",
      "when": [ { "check": "disabled", "target": "sauna_steam_a" } ],
      "actions": [ { "action": "enable", "target": "sauna_steam_a" } ] } ] }
```

### Weather: light snow and blizzard

Weather is optional reusable level content. Add `"weather": {"kind": "snow"}`
to any level for gentle camera-centred snowfall. Winter opts in normally; all
other shipped Places keep no weather. No map-id check exists in the renderer.
Unknown weather keys/kinds fail validation. The configuration travels in
compiled semantics and its material/PNG travels through ordinary dependencies.

| Field | Default | Contract |
| --- | --- | --- |
| `kind` | required | `"snow"` |
| `count` | 1400 | 1–2048 seeds; High/Medium/Low evaluate 100%/75%/50% of a stable prefix |
| `radius` | 16 | 4–32 m spherical draw range in a camera-local horizontal tile |
| `height` | 12 | 4–24 m vertical tile, centred on camera Y |
| `wind` | `[0.18, 0.06]` | finite world X/Z velocities; ±0.5 m/s for calm, ±20 m/s when severity > 0 |
| `size` | `[0.025, 0.075]` | ordered size range, 0.005–0.15 m |
| `speed` | `[0.45, 1.05]` | ordered downward speed range, 0.1–2 m/s |
| `opacity` | 0.85 | finite 0–1 multiplier |
| `intensity` | 1 | finite 0–1 seed fraction, independent of fog |
| `storm_severity` | 0 | finite 0–1; 0 retains calm rendering, 1 is whiteout |
| `visibility_m` | 5 | finite 2–100 m; at severity 1 removes 98.2% of outdoor contrast at this distance |
| `fog_color` | `[0.68, 0.73, 0.79]` | three finite display-space 0–1 components |
| `material` | `core:snowflake_01` | catalog material with a full-UV flake PNG (§7.1a of the asset specification) |

A level may additionally author `weather_alternate` using this same configuration
contract. It requires `weather`; both configurations and material dependencies are
validated and prepared once. `toggle_weather` swaps them for the current session,
including snow motion, storm extinction and sky. Shared effect buffers reserve the
larger seed budget once; selection never reloads or rebakes the map. Live graphics
changes preserve the selection and clock. A fresh load or `reset_to_start` restores
`weather`; no weather state is written to saved settings. A switch may pair state
conditions with `toggle_weather`, `toggle_animation`, `set_prompt` and `set_state`
for distinct on/off feedback through ordinary E key edges.

Continuous controls require matching seed counts and trimmed material ids in both
endpoints. `set_weather_strength` blends numeric snowfall, wind, fog, visibility
and sky parameters in place, retaining seeds, textures and shelter coefficients.
Exact strength 0 restores the authored normal configuration. These controls and
the optional timer advance only during gameplay; pause and loading freeze them.
Live graphics changes preserve playback. Manual strength/toggle actions cancel
cycling until an explicit `set_weather_cycle` enables it again.

Optional `weather_cycle` requires both compatible endpoints. Its fields are:

| Field | Default | Contract |
| --- | --- | --- |
| `enabled` | false | start automatically on fresh load when true |
| `min_strength` / `max_strength` | 0 / 1 | finite 0–1, minimum strictly below maximum |
| `period_seconds` | 60 | finite 1–3600 seconds; includes two equal dwells and two ramps |
| `transition_seconds` | 4 | finite 0–300 seconds, no greater than half the period |

Enabling first eases from the current strength to the minimum, then begins a
minimum dwell, rising ramp, maximum dwell and falling ramp. A zero-duration ramp
switches at the dwell boundary. Reload or `reset_to_start` restores authored
weather and the authored cycle-enabled setting; nothing is saved to preferences.

Winter's timber-backed panel is inside the raised lodge, east of its entrance:
world X/Z `(-10.25, -10.34)`. Enter up the front ramp, turn toward the door's east
jamb, approach and press **E** while aiming at a rocker. The large **Blizzard
control** centred at world Y 1.89 m alternates moderate strength 0.35 and calm over
two seconds. Its prompt is “Enable moderate blizzard” / “Stop blizzard”. Three
lower rockers select mild 0.15, moderate 0.35 or severe 1.0 over two seconds.
The upper **Timed snowfall** rocker starts/stops the optional 120-second cycle
between 0 and 0.45, with 15-second ramps and 45-second dwells. Stopping eases to
calm; any manual control cancels the timer. Fresh Winter is calm with the timer
off. The alternate retains the existing severe review's wind `[8, 3]`, 6 m snow
radius and 5 m visibility. Ceilings and porch slabs retain shelter protection.

Flakes fall downward with independently seeded speed, size and sinusoidal drift.
Their world positions stay fixed under camera translation until an invisible
volume boundary wraps. Smooth near, radial and vertical fades suppress popping;
only flakes inside the actual view frustum are uploaded. One depth-tested blend
draw shares the existing ambient-effect pipeline. No lighting bake, particle
spawn queue, per-frame seed/scratch allocation or map-wide particle simulation
is added. The maximum steam plus snow budget is 10,240 quads, within `u16`
indices; buffers reserve only the actual level budget, including disabled steam.

All non-open room ceilings, including gables, automatically shelter their
footprints below the ceiling. Authored occluding `void_walls` shelter below their
top faces too, covering slab roofs/porches. A 0.35 m outer boundary fade prevents
flakes appearing abruptly as drift crosses a footprint. Outdoor snow remains
visible through an open doorway/window from indoors. This intentionally small
shelter model does not trace arbitrary prop roofs, tree canopies or dynamic
geometry; author a ceiling or roof slab when those need weather shelter.
Snow is presentation only, without collision or accumulation. Ground clipping
uses the world's existing depth buffer.

For severe weather, keep the seed budget small and use a compact nearby volume:

```json
"weather": {
  "kind": "snow", "radius": 6, "height": 6,
  "wind": [8, 3], "size": [0.025, 0.06], "speed": [0.8, 1.8],
  "storm_severity": 1, "visibility_m": 5
}
```

Storm extinction is exponential-squared in the **outdoor portion** of each
camera-to-fragment sightline, after the normal atmosphere. It affects world
surfaces, props, characters, translucent ice, decals and emitted bloom. Nearby
warm lamps glow; distant emissive sources disappear rather than leaking bright
bloom through the whiteout. The background sky blends toward the same fog color
by the square root of severity, hiding aurora at full severity. No illumination,
lightmap or reflection bake changes. Storm visibility stays identical across
qualities; only the particle seed prefix changes. Strong wind gives velocity-
aligned billboard streaks using the existing flake PNG and depth testing.

Room interiors use their actual convex flat/gable roof volumes; occluding
`void_walls` shelter the space below their top faces. Ray intervals are merged,
so overlapping eaves/rooms never double-subtract optical distance. Indoor
sightlines stay clear while views through openings fog over the exterior part
of the ray. Storm maps may contain at most 32 combined non-open room ceilings
and occluding roof slabs; both validators reject excess rather than silently
losing shelters. As for snow, arbitrary prop/dynamic roofs are not traced.
There is no screen-space noise or veil and no new texture/camera shake effect.

Winter keeps its calm default. The separately compiled
`debug-maps/blizzard-20261007/sources/blizzard_review.json` is a severe Winter
review configuration, preserving all original terrain, buildings, pond, trees,
lights and sky. Its README provides compilation, native captures and launches.

Benchmark diagnostics: with `PLACES_BENCH=1`, `PLACES_WEATHER_TRACE=<csv>` records
camera, evaluated/submitted/sheltered/culled counts, sum of projected quad areas
as a screen fraction, scratch capacity growth and billboard-sync CPU µs.
Coverage is an upper estimate before texture alpha/depth, not a hardware fragment
counter. `PLACES_BENCH_WEATHER_OFF=1` disables only weather for same-binary A/B
measurements. Both switches are inert in normal play. Use native camera movement
and High/Medium/Low captures; CPU tests alone do not prove appearance.

### Solid glass

A `glass` opening may also author `"solid": true`: the pane then blocks the
player as a thin collision slab while still drawing in the transparent blend
pass. Rendering transparency and physical collision are independent — the
material's `alpha_mode` never decides collision, and `solid` never changes how the
pane draws. `solid: true` requires `glass` (an invisible solid barrier is a wall,
not an opening). All shipped windows and the transfer grille
author `solid: true`; a purely decorative pane leaves the default `false`.

```json
{ "kind": "window", "offset": 3.05, "width": 2.2, "height": 1.4, "sill": 1.7,
  "glass": "core:glass_tinted_01", "solid": true }
```

A solid pane is the right way to close a window or a glass partition; a
walk-through pane is the right way to read a doorway as glazed without blocking
the route.

---

## 31. The Map Geometry Checker

`places --check-geometry` is a read-only CLI over the same interpretation the
game builds: it parses and validates a level, runs the same preparation pass
(fixture snapping, ceiling-decal snapping, automatic baseboards), emits the
static mesh and derives collision, then reports what is measurably wrong.

```text
# Human report (errors and warnings)
cargo run --release -- --check-geometry --level assets/levels/places_demo.json

# Machine-readable report and marker files
cargo run --release -- --check-geometry --level levels/level0_pit.json \
    --json target/report.json \
    --markers target/markers.json \
    --markers-obj target/markers.obj

# Warnings fail the run too (for CI)
cargo run --release -- --check-geometry --level places_demo --strict
```

| Flag | Meaning |
| --- | --- |
| `--level <path-or-id>` | A level JSON path, `places_demo`, or a drop-in id under `levels/`. |
| `--json <path>` | Writes the machine-readable report (format `places-geometry-check`, version 1). |
| `--markers <path>` | Writes marker anchors as JSON. |
| `--markers-obj <path>` | Writes a small cross per finding as an OBJ, for visual localization. |
| `--strict` | Treat warnings as a failing status. |
| `--quiet` | Human report lists errors only. |

Exit statuses: **0** no confirmed defects, **1** confirmed defects (or any
warning with `--strict`), **2** usage or file/parse failure.

Each finding names its check, severity, element id, message and a world
position. Checks include:

| Check | Severity | What it proves |
| --- | --- | --- |
| `level-invalid` | error | The loader rejected the level; the reason is quoted. |
| `degenerate-face` | error | A zero-area or repeated-corner triangle in the emitted mesh. |
| `non-finite-vertex` | error | A generated position, uv, colour or frame is not finite. |
| `duplicate-surface` | error | Two coplanar triangles (over 10 cm²) overlap, e.g. duplicated trim. |
| `coplanar-sliver` | warning | A sub-10 cm² overlap sliver at a joint; reported, not hidden. |
| `reversed-face` | warning | Two coplanar faces overlap with opposite normals; could be a genuine reversal or a legitimate back-to-back pair (two stacked walls' caps share a junction plane, and the checker cannot tell them apart without solid semantics). |
| `overlap-emission` | warning | Two rooms' floors/ceilings share a plane (intentional overlaps emit both). |
| `collision-duplicate` | error | Two authored solids share an identical collision box. |
| `collision-mismatch` | error | An authored solid is missing from the engine's collision set. |
| `ghost-collider` | warning | A surface-tight rectangular collider (a wall slice, half wall, column or archway) with no mesh surface on any face. Guardrails (their barrier box deliberately reaches below the rails) and curved primitives (whose row AABBs over-cover by construction) are exempt. |
| `prop-duplicate` | warning | Two placements of one model share the same transform (position, yaw and scale within tolerance); the copy is redundant and z-fights. |
| `prop-layer-coplanar` | warning | A prop's visible top sits within 2 cm of the walkable floor at its own footprint while its base does not (a large dressing slab stacked almost on the real surface, e.g. a kit underlay or porch deck left at the floor plane). Props under 0.25 m² of footprint are exempt: a tiny fixture cannot hide a floor's worth of flicker. |
| `opening-overlap` | error | Two openings in one wall overlap; they resolve as one merged hole. |
| `opening-unused` | warning | An opening does not intersect the wall solid; it cuts nothing. |
| `curved-invalid` | error | A degenerate arc wall/pillar (radius, thickness, sweep, segments). |
| `curved-collision-gap` | error | Collision does not cover the drawn curved polygon. |
| `curved-collision-overshoot` | error | Collision reaches grossly past the drawn curve. Only checked on a curve whose tessellation is already fine (sagitta ≤ 2 cm); a coarse tessellation gets the actionable `curve-coarse` warning instead, because its AABB slack follows from the tessellation itself. |
| `curve-coarse` | warning | A curve's tessellation leaves a sagitta above 2 cm. |
| `missing-wall` | warning | A room perimeter run has no wall solid and no authored opening. |
| `room-leak` | warning | A room's walkable space reaches the *void* (outside every room). |
| `spawn-outside-room` | warning | The spawn is outside every room (initial height uses y = 0; movement has no supporting floor). |
| `wall-joint-step` | error | Two end-to-end wall slices of equal thickness are shifted by the same amount (rigid, > 1 mm, ≤ 0.25 m): a doorway side sliver or wall section placed half a thickness off the adjoining wall face. Auto-repairable. |
| `wall-joint-step-review` | warning | A rigid shift above the automatic limit, an ambiguous authority, or a small coplanar gap not filled by either wall's own solid: manual review, never moved automatically. |
| `wall-joint-thickness-step` | warning | One thickness face is aligned and the other is not: a valid thickness transition or a misplaced sliver. Manual review. |
| `wall-joint-emitted-mismatch` | error | The emitted mesh disagrees with the source decomposition at a joint (for example a coalesced wall snapped onto another plane, or an un-declared step). A generator defect. |

**Wall-joint checks.** Besides the checks above, the checker verifies wall-plane
continuity at doorway and wall junctions. The target is *intended architectural
continuity*, not universal coplanarity: jambs, reveals, trim, thickness
transitions, parallel partitions and deliberate offsets are preserved. A **wall
joint** is a pair of solid wall slices (the same decomposition the emitter uses,
openings removed) that are on the same axis, end-to-end (touching within 5 cm,
or separated by at most 35 cm), vertically overlapping by at least 30 cm, not
overlapping along their length by more than half the shorter slice (an overlay
such as a notice board is not a joint), the same thickness within 1 mm (the
emitter's own coincidence tolerance), and shifted so that both thickness planes
move by the same rigid amount. A joint only qualifies when both plane deltas are
within 35 cm, so unrelated parallel walls are never paired.

The wall that keeps its plane is the one with more directly touching coplanar
neighbours (a verified continuous wall); ties go to the longer solid span, then
to the lower authored index. A tie on both is review-only. Nearness, width or
matching material alone never establishes continuity, and the *main* wall is
never moved to accommodate a bad sliver. Tolerances: plane 1 mm, adjacency 5 cm
(strict) / 35 cm (review), vertical overlap 30 cm, largest automatic shift
25 cm, repairs quantised to 0.1 mm.

**Repairing a joint.** A confirmed `wall-joint-step` is repaired with the
read-only planner and the order-preserving applier, both offline:

```sh
# 1. Plan (read-only; never writes the map).
./target/release/places --repair-geometry --level assets/levels/places_demo.json \
    --plan target/demo-plan.json

# 2. Verify the plan against the file on disk (hash and old values).
python3 tools/levels/repair_alignment.py --plan target/demo-plan.json --check

# 3. Apply atomically, then re-check and prove a second plan is empty.
python3 tools/levels/repair_alignment.py --plan target/demo-plan.json --apply
./target/release/places --check-geometry --level assets/levels/places_demo.json
```

A high-confidence repair moves only the authored wall that lost the authority
decision by the measured rigid shift, plus the wall's own coupled elements:
skirting parallel to the moved face moves with it, a run end tucked into the
face moves, and a floor region that tucks under the face is extended. Anything
else within 15 cm of the moved faces is reported as `coupled-review` and never
moved automatically. The applier refuses a file whose hash or field values
changed since planning, writes through a temporary file with an atomic replace,
validates the result with the checker, restores the original bytes on any
failure, and proves idempotence by re-planning. Repairs are an offline
authoring action: the player never snaps or repairs geometry at load time.

**Intent annotations.** A heuristic warning that is deliberate is suppressed by
a narrow `geometry_intent[]` rectangle with the check id and a note — never by
widening a threshold or suppressing a check globally. Errors are never
suppressed:

```json
"geometry_intent": [
  { "check": "missing-wall", "x": 68.9, "z": 2.7, "width": 1.75, "depth": 0.6,
    "note": "The corridor loop's return leg opens into the stair hall here." }
]
```

**Honest limitations.** The checker reads the level geometry, not the rendered
image. It is not a watertightness proof: intentional doorways, windows,
stair openings, pools, carpet holes and open-plan edges are legitimate, and the
leak check only flags escapes into the void. It sees the asset-less mesh (prop
placeholders, no GLB interiors) and does not validate prop models, textures or
shading. Marker files and `--json` are the reproducible localization path for
every finding; a clean run does not prove that no geometry defect exists.

The ghost-collider warning tests actual coplanar triangle/face overlap and a
covered face centre, rather than requiring the triangle centroid inside the
collision partition. A large triangle may support a small legitimate wall tail.
Separate coplanar fragments cannot fill a real centre hole through their bounds.
This remains a support heuristic: one supported face centre is sufficient, and
conservative roof-following boxes need no invented horizontal cap. It does not
prove full face coverage or watertight collision.

---

## 32. The Model Zoo and the capacity fixtures

`assets/levels/model_zoo.json` is the generated showroom: one large, well-lit
pool hall (pool deck floor, pool wall tile, pool ceiling, a grid of pool
downlights) that displays **every registered placeable model** at least once,
plus the demonstrations that need more than one copy. It is bundled with the
game and appears in Level Select as **Model Zoo**.

### What it demonstrates

* one display of every `prop` and `entity` entry in `assets/catalog.json`,
  grouped by display class (floor standing, wall mounted, ceiling suspended,
  tabletop, water, posed, routed);
* three concrete-mannequin poses (`pose_stand`, `pose_arms_up`,
  `pose_arms_forward`) and three skeleton poses (`pose_stand`, `pose_sit_floor`,
  `pose_sit_chair`), each held as its rest pose by a one-step route and
  replayable through its own `play_animation` binding;
* a skeleton chair pose seated on a real `core:chair`, placed with the offset
  documented in `assets/entities/skeleton/README.md`;
* a walking rat and a running rat on separate routes at their measured
  reference speeds, and two independently routed Spooner-Man instances (one runs
  the full `walk → sit_down → sit_idle → stand_up` sequence);
* two wall switches with independent `toggle_animation` state, plus the stop
  sign, the illuminated green exit sign, the hanging white ball light and the
  CRT television, each on a real mount and (for the two luminous props) with its
  own authored light so the room actually gains light from them;
* the table setting (knife, fork, spoon, plate, bowl and potted plant) on a real
  `core:table`, the CRT on its own media table, and the yellow duck floating in
  a contained basin with a real ladder volume;
* two curved walls and two circular pillars in more than one material, and
  ceiling vent decals snapped to the room's own panel grid;
* the generic-spawn demonstration: the crate display's `interact` binding runs
  `spawn_entity` for a half-scale `core:crate` template (20 s lifetime, a
  `phase` state) at a floor spawn point in an `at_most_one_active` group, so a
  second press while the crate is alive is refused with a diagnostic.

### Generating and checking it

```sh
python3 tools/levels/build_model_zoo.py              # write the level
python3 tools/levels/build_model_zoo.py --check      # fail (exit 1) if stale
python3 tools/levels/build_model_zoo.py --stats      # coverage + layout summary
python3 tools/levels/build_model_zoo.py --workers 8  # bound the inspection pool
PLACES_TOOL_WORKERS=4 python3 tools/levels/build_model_zoo.py
```

* **Catalog-driven.** The display list is derived from `assets/catalog.json` and
  the models' real bounds; there is no hand-written inventory. Adding a catalog
  entry adds a display on the next run, removing one removes its display, and
  neither renumbers any other instance.
* **Stable ids.** Every display is `zoo:<catalog-id>:<role>` (for example
  `zoo:core-desk:floor`, `zoo:mannequin:arms-up`). Reordering the catalog array
  changes nothing; the id is a function of the asset id and the role only.
* **Deterministic.** No timestamps, no absolute paths, no worker-order effects:
  serial and parallel runs write byte-identical output, and re-running with an
  unchanged catalog is a no-op.
* **Bounds-aware.** Floor rows are derived from the largest real footprint plus
  the clear-aisle rule; the hall grows a row (and its fixture grid) as content
  grows; animated displays are placed in a reserved lane so a route can never be
  blocked by an exhibit.
* **Baked within budget.** The continuous hall uses flat ownership cells at
  most 16 × 18 m so its floor and ceiling charts pack within the current eleven-page
  Full budget. Shared cell borders are open, with narrow checker intent
  strips; the exterior shell remains enclosed apart from its authored exit.
  The basin fits within one cell. Thin snow attachments and icicles mount on
  the display wall, and all three string spans hang from the ceiling with their
  own amber sources. These mounts preserve each model's native size.
* **A real cache.** Model inspection (rest bounds, clip metadata and the sampled
  animation envelope) is cached under `cache/zoo_inspection.json`, keyed per
  model by path, file size and mtime plus the tool's cache version. A changed
  model or clip invalidates only its own entry; `--no-cache` bypasses it.
* **A real check.** `--check` re-derives the level and compares it
  byte-for-byte, reports the missing/extra display ids when they differ, and
  exits non-zero. `tests/test_zoo_generator.py` additionally drives the
  generator against isolated growth and removal catalogs.

### Capacity fixtures

`tools/levels/build_capacity_fixtures.py` generates the maintained stress fixture
the raised limits are measured against:

| Fixture | What it binds |
| --- | --- |
| `capacity_dense` | 5000+ placements using a fixed historical 48-model witness, 100+ fixtures, 18 routed entities and a real basin, in one 76 m x 56 m hall. Proves the raised instance/model/vertex/fixture budgets and gives the collision index its dense witness set. Model Zoo owns complete current catalogue coverage; catalogue growth must not erase this fixture's collision pressure. |
| `capacity_beyond_former_limits` | The 2026 pass's high-count source: 20 001 walls, 20 001 props, 2 025 rooms, 2 001 floor regions / patches / water volumes, and 81 582 distinct material ids (past the former 16-bit index boundary). It compiles offline to a 45.4 MiB package and is loaded through the package reader; see the Level limits section. |

The generated fixtures support `--check`; the dense fixture must also pass
`places --check-geometry` with no errors, and `src/zoo_audit.rs` pins its
contracts while retaining an in-memory large-coordinate regression for the
retired sparse source. The beyond-former-limits source is a capacity fixture,
not a design-reviewed map: its rooms intentionally open onto the grid, so it is
compiled and loaded through the package path rather than audited for leaks.

---

## 33. Navigation and AI

**The player never bakes navigation.** Every installed package carries a baked
navigation mesh (`.navigation`), produced offline by `places-compile` from the
level's own walkable surfaces and its compiled collision. A package without the
record is refused by name, and a malformed record fails the load; there is no
runtime bake, repair or fallback.

### 33.1 What the bake derives

* **Walkable surface.** The same `WalkableFloor` the movement controller
  follows, so ramps and stair pitch are sampled exactly as a player walks them.
  Off-room cells and deep water (a `swimming` volume whose surface is more than
  one step above the floor) are not walkable.
* **Bodies and classes.** The bake writes one **agent class** per distinct
  `nav_agent` body in the map, plus the reference humanoid
  (`radius 0.30`, `height 1.8`, `step_height 0.4`). A cell marked walkable for a
  class is physically traversable by exactly that body: the class's disc does
  not touch a blocking box in its body band, its head fits under the ceiling
  and any overhead, and its step/slope limits connect it to its neighbours.
  At most 8 distinct bodies per map; a ninth is a compile error.
* **Obstacles.** Walls, architecture and solid props come from the compiled
  collision boxes, so a solid prop blocks navigation exactly when it blocks a
  player. A `nav_obstacle` component with `affects_nav: true` adds an explicit
  box (rotated footprints are covered conservatively), and the compiler's
  `NavWalkProxy` input can add walkable surfaces for future collision proxies.
* **Stairs and slopes.** A neighbour connects when the surface rises at most
  `step_height`, or when both cells lie on one **continuous slope** — a cell
  whose surface gradient is consistent across it, as a ramp or a staircase's
  pitch line is — and the rise per metre is at most `max_slope` (default
  `2.6667`, the worst authored stair pitch
  `MAX_STAIR_RISER_M / MIN_STAIR_TREAD_M`; ramps are bounded lower). A
  discrete riser (a floor-region step, a stair's first nosing, a ledge) is
  never a continuous slope, so the class's own `step_height` alone decides it:
  the Demo's 0.3 m landing steps connect the 0.3 m and 0.4 m walkers and
  refuse the 0.2 m rat, and the 0.2625 m home staircase connects every body
  whose step reaches its riser. A cliff between two flat cells never connects.
  The shared mover in `src/ai/movement.rs` advances in
  `NAV_MOVE_SUBSTEP_M` (0.10 m) substeps and allows a rise of
  `step_height + max_slope * substep` in one substep. That per-substep budget
  is at least the bake's step rule, so a link the bake writes is walkable
  along its own axis; one documented corner case remains: a *diagonal* link
  whose straight substep crosses a concave dip the bake sampled as two
  separate risers can still be refused by the mover (the Demo's pool-landing
  corner has two such undirected pairs out of 117730 directed links, on cells
  no class can plan onto). Routing is the bake's decision, and a refused
  substep is a `Blocked` move, never a fall or a teleport.
* **Doors are portals, not walls.** The cells a leaf sweeps are recorded as
  that door's portal. A closed or locked door blocks them for a route; an open
  door passes; an agent whose `ai` sets `can_open_doors: true` plans through an
  unlocked closed door and asks for it on approach. Opening a door never
  rebuilds the mesh.
* **Connectivity.** Walkable cells are labelled with connected region ids per
  class. Nearest-point queries never cross a region boundary (so nothing snaps
  through a wall, a locked door or a floor), and flee scoring prefers larger
  regions to avoid blind dead ends.

### 33.2 `nav_agent` — the physical body

```json
{ "component": "nav_agent", "radius": 0.2, "height": 0.45, "speed_mps": 1.9,
  "step_height": 0.3, "max_slope": 2.6667 }
```

| Field | Default | Meaning |
| --- | --- | --- |
| `radius` | required | body disc radius in metres; the baked clearance test |
| `speed_mps` | required | preferred speed for non-AI movement metadata |
| `height` | `1.8` | standing body height; headroom and the blocking band |
| `step_height` | `0.4` | largest surface rise walked without a route |
| `max_slope` | `2.6667` | largest walkable rise per metre of run |

`radius`, `speed_mps`, `height`, `step_height` and `max_slope` must all be
finite and positive. An entity that authors `ai` **must** also author a
`nav_agent`: an agent whose body has no baked class can never move.

### 33.3 `ai` — the behavior

```json
{ "component": "ai", "behavior": "predator", "role": "predator",
  "reacts_to": ["prey_rat"], "walk_speed": 0.55, "run_speed": 1.9,
  "sight_range": 10.0, "sight_fov_degrees": 220.0, "hearing_range": 9.0,
  "pursue_distance": 20.0, "catch_radius": 0.5, "catch_height": 0.7,
  "can_open_doors": true }
```

| Field | Default | Meaning |
| --- | --- | --- |
| `behavior` | `idler` | `idler`, `wanderer`, `prey`, `predator` or `follower` |
| `role` | — | tag this agent advertises (`prey_rat`, `predator`, …) |
| `reacts_to` | `[]` | role tags this agent reacts to |
| `walk_speed` | `1.0` | ordinary walking speed, m/s |
| `run_speed` | `2.0` | flee/pursue speed, m/s |
| `sight_range` | `8.0` | metres; `0` disables sight |
| `sight_fov_degrees` | `200` | full horizontal field of view (`0..=360`) |
| `hearing_range` | `6.0` | metres; `0` disables hearing |
| `flee_distance` | `5.0` | preferred separation a fleeing agent seeks |
| `pursue_distance` | `12.0` | a predator gives up beyond this range |
| `catch_radius` | `0.45` | reach added to the target's radius, in metres |
| `catch_height` | `0.8` | largest vertical separation a catch accepts |
| `wander_radius` | `4.0` | wander destinations stay inside this radius |
| `idle_seconds` | `2.5` | hold time between wander destinations |
| `can_open_doors` | `false` | may open an unlocked door on its route |

Behavior is data, never a model or map name: a `prey` flees every agent whose
`role` appears in its `reacts_to`; a `predator` pursues and catches such an
agent; a `follower` keeps one in sight; a `wanderer` picks navigable
destinations around its post; an `idler` stands and reacts.

Perception is evaluated on a staggered interval per agent (0.2 s), never as a
global per-frame scan. Sight tests range, FOV, vertical separation and a real
collision ray against walls and live door leaves. Hearing reads **gameplay
stimuli** — movement noise (released by any agent above 0.55 m/s), door
open/close, switch interaction, spawns and sound actions — each with position,
radius, loudness, category and source, so a predator can hear a rat it cannot
see and the rat can hear the cat running behind it.

### 33.4 States, catching and animation

The state machine is `idle`, `wander`, `follow`, `flee`, `investigate`,
`pursue`, `catch`, `scripted` and `caught`. `ai_state` events are emitted on
every transition (key = the state name) and `caught` fires **once** when a
predator genuinely reaches a live, visible target on its own floor — never
through a wall or across a floor. A catch freezes the target; the map's
`on: "caught"` binding typically starts a sequence on the predator for the
pounce/consume presentation.

Locomotion maps to the ordinary pose vocabulary: idle, walk/run (scaled by
actual speed), so the same clip set placed characters already use. While an
agent runs a sequence the AI yields locomotion (`scripted`); while it is
catching or caught, a `play_animation` override owns the pose. Once ordinary
locomotion resumes, the state-driven gait wins again so a finished one-shot
cannot pin an agent in a held pose.

### 33.5 The home encounter (the worked example)

`assets/levels/places_demo.json` authors the complete encounter with ordinary
data only:

* `rat_release_switch` is a `home:wall_switch` beside the hall door whose
  `interact` binding runs `toggle_animation` and
  `spawn_entity { point, group, name }`;
* the group `home_rat_encounter` is `at_most_one_active`, so repeated presses
  while a rat is alive are refused and the group releases when the rat
  despawns;
* template `home_rat` is a 0.1 m radius, 0.16 m tall `nav_agent` with
  `behavior: "prey"` that reacts to `predator`;
* `spooner_man_home` is a cat-sized `nav_agent` (0.2 m radius, 0.45 m tall,
  `run_speed 2.2` against the rat's `1.75`) with `behavior: "predator"` and
  `can_open_doors: true`; the speed difference plus the real topology makes the
  chase end in a catch in roughly ten seconds of simulated time;
* his `on: "caught"` binding starts the `caught_prey` sequence: `play_animation
  pounce` → `wait_animation` → `sit_down`/`sit_idle` consume presentation
  (the rig has no dedicated eat clip, so the authored sit clips carry it) →
  `despawn_entity` on the **spawn group** at the authored completion point →
  `stand_up` → idle.

Nothing in the engine names the rat, the cat or the map: the switch, the
template, the group, the tags and the sequence are all ordinary data.

### 33.6 Debugging navigation and AI

* `NavMesh::debug_ascii(class, doors)` renders the live walkable grid, region
  membership, portal cells and blocked portals as text, from the same cells
  the queries use.
* `EntityWorld::ai()` exposes every agent's state, current path, flee
  destination, pursuit target, contact and speed. `EntityWorld::stimuli()`
  lists the live hearing stimuli.
* `PLACES_NAV_DEBUG=<dir>` writes the live mesh as `navmesh-class<N>.txt`
  (with the blocking portal cells marked) and `ai-state.txt`, refreshed every
  half second: per agent the state, position, speed, path goal, waypoint count,
  door requests, contact and catch radius, plus the framed stimulus, active
  sequence and aimed-interactable summary. `PLACES_VERBOSE=1` prints the AI
  summaries the frame loop reports (unplaced agents, refused spawns, door
  requests).

### 33.7 Navigation and AI interfaces

* Stair/ramp walkability is one rule: `nav::neighbour_rule` (step or
  continuous slope) plus the movement invariant
  `NAV_MOVE_SUBSTEP_M * NAV_MAX_SLOPE <= step_height`, pinned by a test. A
  staircase that validates in the loader bakes and walks; if a future
  collision proxy is added, feed it through `NavBakeInput::walk_proxies` or
  `NavBakeInput::obstacles` rather than a second format.
* `nav_agent` and `ai` are the only actor components; do not add per-model
  navigation or animation special cases.
* The compiler reports a build warning naming any `nav_agent` entity or spawn
  point with no navigable cell, so a stair or passage that silently breaks an
  actor's clearance is visible at build time.

## 34. The Outdoor Kit

The `outdoor` theme is a reusable night-exterior kit: tileable ground
materials, three grass densities, a dirt/gravel path with feathered
transitions, a concrete walkway, a leafy tree, an exterior lamp family with a
fence post, modular house facade parts and a faint-star night sky. Every id in
this section is a real `assets/catalog.json` entry.

### 34.1 Ground materials

| Material id | Texture file | Texture id | `tile_metres` | Use |
| --- | --- | --- | --- | --- |
| `outdoor:grass_ground_01` | `grass_ground_01.png` | `outdoor:tex_grass_ground_01` | 2.0 | lawns, verges, the ground under a scattered field |
| `outdoor:dirt_gravel_01` | `dirt_gravel_01.png` | `outdoor:tex_dirt_gravel_01` | 1.6 | the walked dirt/gravel path |
| `outdoor:concrete_pavement_01` | `concrete_pavement_01.png` | `outdoor:tex_concrete_pavement_01` | 2.0 | walkways, porches, foundations |
| `outdoor:house_siding_01` | `siding_01.png` | `outdoor:tex_house_siding_01` | 1.2 | exterior cladding on real `walls` |
| `outdoor:house_roof_shingle_01` | `roof_shingle_01.png` | `outdoor:tex_house_roof_shingle_01` | 1.0 | roof slopes and a gable room's `ceiling_material` |

Assign them like any material: `rooms[].material` / `defaults.floor` for the
base ground, and `floor_patches[]` for paths and walkways. A floor patch is a
**material region in the room's own floor grid**, not a second coplanar quad:
the grid resolves one material per cell (later patches win), so a path meets
grass with no z-fighting and no duplicate surface. Nothing about a floor
material adds collision or navigation.

```json
"rooms": [ { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 18.0, "height": 5.0,
             "ceiling": { "kind": "open" },
             "material": "outdoor:grass_ground_01" } ],
"floor_patches": [
  { "x": 1.0, "z": 8.0, "width": 20.0, "depth": 1.6,
    "material": "outdoor:dirt_gravel_01" },
  { "x": 16.0, "z": 0.0, "width": 3.0, "depth": 8.0,
    "material": "outdoor:concrete_pavement_01" }
]
```

An exterior room authors `"ceiling": { "kind": "open" }` (§7): the ground is a
normal room floor, and the sky shows above. An exterior floor with no room at
all does not exist — the room is what gives the ground a surface and the
lighting a volume.

### 34.2 Grass and the three densities

Two alpha-cutout tuft models:

| Asset id | Size `[w,h,d]` m | Notes |
| --- | --- | --- |
| `outdoor:grass_patch_small` | `[0.55, 0.30, 0.55]` | low tuft, the LOW field's main body |
| `outdoor:grass_patch_large` | `[0.95, 0.62, 0.95]` | taller cluster for MEDIUM and DENSE fields |

Both draw through the alpha-tested cutout pass (glTF `MASK`, cutoff 0.5), are
non-solid by default, and are authored with `"occludes": false` so the light
bake never grinds solid shadow boxes out of blade cards.

**Densities are authoring choices, not graphics settings.** They are three
instance counts on the ground, chosen per area; the Low/Medium/High quality
tiers never change the number of tufts, they only change what each tuft costs
(Low uploads prop sheets at 128 and runs the vertex-lit path without
lightmaps). An intentional dense field stays dense at every tier.

`tools/levels/scatter_grass.py` emits the placements deterministically:

```sh
# Print placements for one area (JSON prop array on stdout).
python3 tools/levels/scatter_grass.py --area 0,0,24,18 --density medium --seed 7

# Write them straight into a level's props array.
python3 tools/levels/scatter_grass.py --area 0,0,24,18 --area 6,2,8,5 \
    --density dense --seed 12 --keep-out 2,8,20,1.6 --target my_level.json --apply

# Prove the file's grass matches what the tool would emit today.
python3 tools/levels/scatter_grass.py --area 0,0,24,18 --density dense --seed 12 \
    --target my_level.json --check
```

| Profile | Instances per m² | Minimum spacing | Model mix |
| --- | --- | --- | --- |
| `low` | 0.5 | 0.45 m | 2/3 `grass_patch_small`, 1/3 `grass_patch_large` |
| `medium` | 1.4 | 0.45 m | half/half |
| `dense` | 3.0 | 0.30 m | 2/3 `grass_patch_large`, 1/3 `grass_patch_small` |

* Every emitted prop carries `"occludes": false`, a yaw (`rotation_degrees`), a
  scale in `0.85..1.15`, and a stable id `grass_<n>`.
* The profile numbers are the generator's cell density, not a promised count:
  keep-outs reject candidates without re-filling, so the measured instances per
  m² lands a little under the profile (measured on the generated fixture: 0.54 /
  1.26 / 3.16). The **minimum spacing and the ordering** are the contracts; a
  band's count is whatever survives its keep-outs at that seed.
* More than one density in a level: pass `--id-prefix grass_<band>_` per call
  (the generated fixture uses `grass_low_`, `grass_medium_`, `grass_dense_`),
  then merge the outputs; the prefix must start with `grass_` so a level-wide
  replace still finds every band.
* The same seed, areas and keep-outs always produce byte-identical output: the
  generator is a seeded LCG and the output is sorted by `(z, x)`. `--check`
  re-derives and reports drift.
* `--keep-out` rectangles keep grass off paths, doorways, spawn points and
  anything the player must cross; the tool never places inside one.
* Placement is render-instanced by the ordinary prop pipeline: every instance
  of a model in a spatial cell becomes one draw, so a dense field costs draws
  per model per cell, not per tuft.

### 34.3 Path transitions: the feather decals

The path itself is a `floor_patches[]` region. Its border is softened with
three **blended decal sheets**:

| Asset id | Sheet | Shape |
| --- | --- | --- |
| `outdoor:decal_path_edge_01` | 256×128 | dirt grain opaque on one side of the strip, fading to transparent across the width |
| `outdoor:decal_path_end_01` | 128×128 | radial fade, alpha 0 at every edge |
| `outdoor:decal_path_corner_01` | 128×128 | two fades meeting at a rounded inner corner |

The catalog entries author `"alpha_mode": "blend"` (§17). Placement recipe:

* `width` is the blend band **across** the seam: overlap the dirt edge by about
  half the decal width, and lay the decal so the fade ends on the grass.
* `height` runs **along** the edge; a working band is 1.5–2.5 m per decal, placed
  end to end along the path. The edge sheet feathers slightly at its own short
  ends too, so overlap neighbouring strips by about 0.2 m (the generated fixture
  steps 1.8 m for a 2.0 m strip) rather than butting them.
* `rotation_degrees` is the edge's direction in the surface plane. One straight
  sheet serves both sides of a path: rotate it 180° for the other side (the two
  rotations differ by 180° so the opaque half always lies over the dirt), or to
  any angle for a diagonal run. The generated fixture
  (`tests/fixtures/levels/outdoor_kit_showcase.json`) is the reference
  placement; its captures validate the orientation.
* Ends and junctions use the end and corner sheets, which is why a bend does
  not need a new material.
* `surface` is `"floor"`, `y` is the floor plane, and the decal is lifted
  0.2 mm and depth-biased, so it can never flicker through the floor or create
  collision (decals have no collider at all).

```json
"decals": [
  { "x": 2.0, "y": 0.0, "z": 7.4, "width": 1.0, "height": 2.0,
    "rotation_degrees": 0.0, "material": "outdoor:decal_path_edge_01",
    "surface": "floor" },
  { "x": 19.4, "y": 0.0, "z": 7.4, "width": 1.0, "height": 2.0,
    "rotation_degrees": 180.0, "material": "outdoor:decal_path_edge_01",
    "surface": "floor" },
  { "x": 21.0, "y": 0.0, "z": 8.0, "width": 1.0, "height": 1.0,
    "rotation_degrees": 0.0, "material": "outdoor:decal_path_end_01",
    "surface": "floor" }
]
```

Blended decals are sorted back to front inside their own pass and depth-test
against everything opaque, so overlapping feather strips composite in a stable
order and can never draw through a floor or a wall.

### 34.4 Concrete walkway

Use `outdoor:concrete_pavement_01` for the walkway region and
`outdoor:concrete_step` where it meets a doorway threshold so the change of
surface is one low step rather than a large lip:

```json
{ "model": "outdoor:concrete_step", "x": 21.0, "y": 0.0, "z": 3.2,
  "size": [1.4, 0.18, 0.7], "solid": true }
```

`solid: true` with the step's real size makes it landable; `size` is what the
collider uses (§19), so keep it equal to the drawn step. The step is ordinary
prop geometry, not a new floor element.

### 34.5 Tree

`outdoor:tree_01` is a substantial stylized tree: tapered branching and
asymmetric closed faceted foliage masses, about 4.6 m wide and 6.4 m tall.
The bare trees use opaque foliage; grass remains alpha-cutout. See asset
specification §8.10 and the Outdoors journal for the October reconstruction.
The **canopy** is the catalog box; collision is the level `size`, so place it
solid with a trunk-sized box:

```json
{ "id": "yard_tree", "model": "outdoor:tree_01", "x": 6.0, "z": 3.5,
  "rotation_degrees": 25.0, "scale": 1.1,
  "size": [0.8, 6.4, 0.8], "solid": true, "occludes": false }
```

* `occludes` is the author's choice here. The default (`true`) bakes a coarse
  solid box for the whole silhouette — trunk *and* canopy — which is a heavy
  blob shadow no leaf card could cast. `false` removes the tree from the bake
  entirely, losing the trunk shadow too. For a decorative night tree either is
  defensible; pick one deliberately and keep it consistent for a group of trees.
* Repeated placements vary with `scale` (0.9–1.15), `rotation_degrees` and the
  model choice; there is no vegetation simulation and none is needed.
* Leaves never become collision; only the authored `size` box does.

### 34.6 Exterior lamps

One housing family, three mounts, plus the fence post that supports the middle
one. Each reconstructed lantern has four tapered panes, corner bars, a
pyramidal hood and a finial. Every pane is an emissive material group; **emission is bloom
only**. The real illumination is a `props[].lights` point the level authors at
the documented offset, because no material ever lights anything (§18). The
closed fixture surrounds its published emitter anchor. Set `occludes: false`
on the lamp placement: coarse fixture occlusion otherwise blocks its own
light. This does not remove authored collision or alter the light profile.

| Asset id | Size `[w,h,d]` m | Mount / origin | Documented light profile |
| --- | --- | --- | --- |
| `outdoor:lamp_stand` | `[0.34, 1.05, 0.34]` | base plate on the floor at `y = 0`; pane faces +Z | `point`, offset `[0, 0.86, 0]` (2.5 cm in front of the pane), color `[1.0, 0.86, 0.68]`, intensity 0.7, range 7.0, `smooth` |
| `outdoor:lamp_fence` | `[0.32, 0.42, 0.36]` | saddle plate underside is the origin; seat it on a 0.12 m square post top (i.e. `y` = the post's top height); pane faces +Z | `point`, offset `[0, 0.30, 0.05]`, color `[1.0, 0.86, 0.68]`, intensity 0.5, range 4.0 |
| `outdoor:lamp_wall` | `[0.30, 0.52, 0.34]` | wall plate in the local `z = 0` plane with its top at local `y = 0.52`; the eave hook sits behind it (`z < 0`) and the lantern hangs in front (+Z) | `point`, offset `[0, 0.18, 0.12]`, color `[1.0, 0.86, 0.68]`, intensity 0.6, range 5.0 |
| `outdoor:fence_post` | `[0.12, 1.05, 0.12]` | base on the floor; flat cap seat at `y = 1.05` | the fence lamp's saddle seats on it |

```json
{ "id": "path_lamp_1", "model": "outdoor:lamp_stand", "x": 4.0, "z": 7.2,
  "size": [0.34, 1.05, 0.34], "occludes": false,
  "lights": [ { "shape": "point", "offset": [0.0, 0.86, 0.0],
                "color": [1.0, 0.86, 0.68], "intensity": 0.7,
                "range": 7.0, "falloff": "smooth" } ] }
```

Distances: one stand lamp every 6–9 m along a path is enough for pools to
overlap at the edges; fence lamps suit 2–3 m post spacing; a wall lamp belongs
under an eave or beside a door at about 2.2–2.6 m above the local floor.

### 34.7 House facade kit

Modular parts in metres, all siding-first with painted trim. They are visual
depth on top of real `walls`: the level owns structure and collision, the props
own the exterior read.

| Asset id | Size `[w,h,d]` m | Notes |
| --- | --- | --- |
| `outdoor:house_wall_solid` | `[3.0, 2.7, 0.24]` | siding panel with base band and corner boards; `solid: true` in the level |
| `outdoor:house_wall_window` | `[3.0, 2.7, 0.24]` | panel with a centered framed window and dark glazing |
| `outdoor:house_wall_doorway` | `[3.0, 2.7, 0.40]` | panel with a real 1.1 × 2.15 m doorway through 0.40 m of depth: jamb, head and threshold surfaces, no collision |
| `outdoor:house_roof_slope` | `[3.4, 1.75, 2.6]` | shingle field on top, eave fascia and soffit at the low edge; origin at the eave's underside centre |
| `outdoor:house_roof_ridge` | `[3.4, 0.22, 0.6]` | ridge cap over the slope joint |
| `outdoor:house_corner_trim` | `[0.18, 2.7, 0.18]` | vertical corner board over panel junctions |

The destination house is assembled from normal level parts:

1. **Structure and collision** come from real `walls` and a room. Give the room
   `"ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": <pitch ×
   half-depth> }` so the interior ceiling is the roof pitch, and use
   `outdoor:house_siding_01` + `outdoor:house_roof_shingle_01` as its materials.
2. **The centered doorway** is a real opening: author the wall with an
   `openings` entry (`kind: "door"`, width 1.1, height 2.15) and a real `doors[]`
   entity there (the `interior` kind reuses the existing white door material and
   frame; §30). Remember `offset` is the opening's **near edge** along the wall
   from its minimum corner (§5): to centre a 1.1 m opening on wall centre `cx`,
   use `offset = cx - 0.55 - wall x`. Then place `outdoor:house_wall_doorway`
   centred on the opening, offset outward by 0.20 m so its jamb depth reads on
   both faces, and put the door's hinge x at the opening's near edge so the
   closed leaf fills the hole exactly. The panel builds the visible jamb depth
   too, so author the leaf's `frame_depth`/`frame_center` to span both it and
   the wall (§30). The panel is non-solid; the wall's solid slices provide the
   collision, and the player walks through both holes.
3. **Windows** are `outdoor:house_wall_window` panels, centered on the window
   axis. They are decorative and solid by default: pair one with a real wall
   opening only when the window should actually be glazed. A dark window needs
   no hole.
4. **The roof** is two `outdoor:house_roof_slope` props meeting at a
   `outdoor:house_roof_ridge`, both non-solid, with the eave at the wall top.
   The slope pitches about 32.5° and **rises towards local −Z**, with the eave
   (fascia and soffit) at +Z and the origin at the eave's underside centre; two
   slopes mirrored 180° about Y meet at the ridge cap, whose apex edge is at
   local `y = 0.22`. The integrated fascia/soffit is the visible eave.
5. **Corners and returns**: `outdoor:house_corner_trim` covers a junction, and a
   run of `outdoor:house_wall_solid` panels closes a side return. The exit
   building and the destination building use the same parts.
6. **Eave lamps**: `outdoor:house_wall_doorway` documents its lamp mounts at
   local `[-1.15, 2.55, 0.20]` and `[+1.15, 2.55, 0.20]` (+Z away from the
   wall). Place an `outdoor:lamp_wall` so its wall plate lands on the mount:
   because the lamp's origin is the bottom of its bbox and the plate sits at
   local `y = 0.52`, a mount at 2.55 means `"y": 2.03` relative to the local
   floor. Add the documented point light.

### 34.7a Second tree, conifer, streetlight, railings and the five facade families

The kit grows with a second and third tree, a tall streetlight, a modular porch
railing and four more complete facade families. Every id below is a real
catalog entry; the assembled reference is
`tests/fixtures/levels/outdoor_kit_showcase.json`.

| Asset id | Size `[w,h,d]` m | Notes |
| --- | --- | --- |
| `outdoor:tree_02` | `[2.0, 6.2, 2.0]` | slender deciduous tree: tapered branching, narrow grouped foliage; trunk collider `[0.6, 6.2, 0.6]` |
| `outdoor:tree_03` | `[3.2, 6.8, 3.2]` | **evergreen/conifer**: tapered trunk under six staggered whorls of closed needle masses; trunk collider `[0.6, 6.8, 0.6]` |
| `outdoor:streetlight` | `[0.5, 6.4, 1.2]` | 6 m tapered post, swan-neck arm reaching +Z, tapered four-pane lantern with a pyramidal hood. Retains light offset `[0, 6.05, 0.43]`; set `occludes: false` so its coarse fixture bounds do not block the emitter |
| `outdoor:porch_railing_straight` | `[1.8, 1.05, 0.12]` | rail runs edge to edge; top rail 0.95–1.05, bottom rail 0.14–0.21, metal balusters |
| `outdoor:porch_railing_corner` | `[0.3, 1.05, 0.3]` | two rail stubs cross on the module point; pair it with a `porch_post` on the same point |
| `outdoor:porch_railing_end` | `[0.3, 1.05, 0.12]` | closed by a capped 0.12 m newel |
| `outdoor:porch_post` | `[0.12, 1.05, 0.12]` | support post; flat cap seat at y = 1.05 |

For N in `01..05`, `outdoor:house_0N_<piece>` repeats **one shared
convention** with five distinct artworks: 3.0 × 2.7 m panels (0.24 m deep,
0.40 m for the doorway), a real 1.10 × 2.15 m doorway with jambs, head and
threshold, eave lamp mounts at local `[±1.15, 2.55, 0.20]` (+Z away from the
wall), a 3.0 × 1.02 m gable panel at the kit's 1.75/2.6 pitch with its base at
y = 0, a roof slope whose **origin is its centre** (local z −1.3..+1.3): the
eave underside is at local `z = +1.3, y = 0` and the ridge edge at
`z = −1.3, y = 1.75`, so placing one at an eave means putting the origin half a
run inboard; a ridge cap with its apex at local y = 0.22; a corner board; and a
porch deck (Z −0.75..+0.75 around its origin) with a porch post (base y = 0,
flat top at 2.30 m).

| family | cladding | trim | window | roof | porch |
| --- | --- | --- | --- | --- | --- |
| `01` | faded cream clapboard | dark green | two-pane vertical | grey shingle | simple stoop |
| `02` | pale blue-grey clapboard | cream | warm four-pane 2×2 | charcoal-blue shingle | covered porch |
| `03` | ochre clapboard | white | tall narrow | dark slate | porch railings |
| `04` | dark green board-and-batten | cream | wide three-lite | near-black | small porch |
| `05` | weathered white shiplap | navy | two small squares | green-grey | deep deck |

Mix and match any family's pieces: the grid, pivots and mounting points are
identical, so a shell assembled from one family has no gaps (the fixture
assembles all five). Houses are visual depth on real `walls`; the level owns
structure and collision.

The October Outdoors reconstruction adds `outdoor:bush_round` and
`outdoor:bush_low` (closed asymmetric foliage), `outdoor:fence_two_rail`
(2 m capped timber module), `outdoor:masonry_pier` (1.2 m cap height,
supporting a fence lamp), `outdoor:porch_canopy` (high edge at local -Z),
`outdoor:road_barrier` (striped decorative rail) and `outdoor:boundary_ridge`
(20 × 10 × 4 m distant closed geology), and `outdoor:campfire_static`
(the existing fire construction baked into a genuinely static asset). They use ordinary prop placement;
no new engine fields exist. Shrubs/ridges opt out of coarse bake occlusion,
and ridges remain outside the established playable containment. See
[asset specification §8.10](ASSET_SPECIFICATION.md#810-outdoors-concept-reconstruction)
and the [Outdoors journal](style-upgrade-20261007/outdoors/README.md).

### 34.8 Night sky

A level may declare one sky background:

```json
"sky": {
  "texture": "outdoor:tex_sky_stars_01",
  "brightness": 1.0,
  "ambient": 0.0
}
```

* `texture` is a catalog `texture` asset whose PNG is an **equirectangular
  2:1** sheet: `u` is yaw (seamless around the horizon), `v` is pitch with `0`
  straight up. Sky sheets must have power-of-two edges and fit within 2048×1024;
  the dedicated sky loader validates this before allocating pixels. High uploads
  up to 2048×1024, Medium 1024×512, Low 512×256. `outdoor:tex_sky_stars_01` is 1024×512, near-black with a small
  number of faint stars in the upper half — no moon, no glow, no horizon.
* `brightness` (`0.0..=4.0`, default 1.0) scales the sheet at draw time. It is
  a visual control, not an exposure: the authored art is the look.
* The sky texture supplies the background, while `ambient` and optional
  `ambient_color` supply escaping-ray illumination. A solid ceiling covers the
  background and blocks these rays. A level without `sky` retains its clear
  colour and zero sky radiance.
* `ambient` (`0.0..=1.0`, default 0.0) is the radiance an escaping ray sees in
  the **prepared lightmap solve** (Medium/High): a faint cool dome fill for an
  exterior with no fixtures, directional — upward faces receive it, downward
  faces almost none. `0.0` leaves the solve bit-identical to a level with no
  sky. Optional `ambient_color` is a finite **linear radiance** RGB in 0–1,
  default `[0.42, 0.52, 0.72]`; incident sky equals this colour × `ambient`.
  The same transport feeds static atlases and entity probes, including bounced
  contributions. Runtime never adds that ambient a second time. Low's existing
  vertex fallback keeps its own historical `0.10` ambient floor and does not
  reproduce the prepared sky distribution. Background `brightness` and final
  exposure are independent of stored energy; optional moon/global illuminators
  retain their visibility/cosine paths.
* The sky is never captured into reflection probes or the planar mirror, and it
  is not fogged: it is an infinite background. It *is* a package dependency:
  editing the sheet invalidates a compiled package like any surface texture.
* To see the sky above an exterior, the ground room must author
  `"ceiling": { "kind": "open" }`. Through a window or an open doorway the sky
  is visible from inside a normal room without any change; it can never leak
  through a solid roof because the ceiling geometry draws over the background
  pass with normal depth testing.

## 35. The night route: Places Demo's outdoor extension

Places Demo keeps its interior; north of the front rooms the level carries a
night route about 30 seconds long, from the front door to a lamplit destination
house. **`tools/levels/build_outdoor_route.py` owns that slice end to end.**
Every element it emits is id-namespaced `night_` (grass `grass_night_`, which
keeps the grass tools' whole-level filters matching); re-running it replaces
the previous slice instead of duplicating it, and `--check` re-derives the slice
and fails on any drift. Edit the constants at the top of that tool, run it,
recompile the demo, recapture.

### 35.1 Layout, in real units

| Element | Authored value |
| --- | --- |
| Front doorway | `walls[0]` (the front rooms' north wall at z = −0.15..0.15) with a `door` opening 1.11 × 2.15 m centred on x = 4.5, and a real interior door leaf. |
| Threshold / yard ground | **world y = 0**, the same floor the front rooms already use; no spawn was moved and no elevation is snapped. |
| Night yard room | x = −0.15..24.15, z = −91.85..0, `"ceiling": { "kind": "open" }`, `outdoor:grass_ground_01`. |
| Gravel route | x = 3.2..5.8 (2.6 m wide), z = −91.6..−0.2, `outdoor:dirt_gravel_01`. |
| Concrete walkway | x = 12.5..14.5 (2.0 m wide), same span: genuinely parallel, laterally offset 9 m. |
| Connector | x = 5.8..12.5, z = −88.5..−86.3: 87.4 m from the threshold, 29.1 s at the shipped 3.0 m/s walk. |
| Destination house | 9 × 5.4 m at x = 9.0..18.0, z = −97.1..−91.7, doorway centred on x = 13.5 (the walkway's centre), gable roof, finished entry room at y = 0. |
| Destination envelope | The room's eave is **solved from the roof plane** (`2.8346 m` at the room edge): each family roof slope is placed half its run inboard of the eave so its eave-underside centre and pitch follow the room's gable plane (the model's own soffit and shingle thickness keep the *visible* underside a few centimetres below the plane, which the cap and wall tops cover), and the ridge cap bridges the two slopes' ridge edges. The front and back walls author the rigid eave height (`2.8346 m`), because they run parallel to the ridge and their top is that constant plane (leaving it to the room lookup would follow the yard's open ceiling at the shared seam). The gable end walls follow the slope to the ridge, so the rake is sealed with no slot; side gable walls stop 2 cm short of the perpendicular walls' inner faces, so no two wall faces are coincident. The 0.24 m stoop is a real floor region; the facade panels, door lamps and porch deck author `y = -0.24` so they meet the yard floor while the step stays walkable. |
| Destination dressing | Front facade and roof use the blue-clapboard family (`outdoor:house_02_*`); a porch deck (a landable 0.24 m stoop, `solid` with its real size) and two posts dress the doorway. The other four families stay in the catalogue and the kit fixture. |
| Containment | Four buried `outdoor:collision_peg` placements: the collider boxes are 0.3 m thick, 3.5 m tall from −0.10, along x = −0.30/24.30 and z = −92.00. |
| Ground | One `void_walls[]` slab (`night_ground_slab`, x −6..30, z −98..3, top y = −0.08) 8 cm under the yard's own floor: looking over a boundary edge shows continuous night ground instead of the void. Non-solid and non-occluding; the buried pegs still own containment. |

The route is measured, not assumed: `src/game/tests.rs` walks the real
`Game::update_player_movement` from the threshold and asserts 27–33 s, a
straight line and a y = 0 floor
(`demo_night_route_is_a_thirty_second_walk_from_the_front_door`).

### 35.2 Invisible containment that does not light, shadow or reflect

The yard is bounded by **buried `outdoor:collision_peg` props**, not walls:

```json
{ "id": "night_boundary_0", "model": "outdoor:collision_peg",
  "x": -0.30, "z": -45.925, "y": -0.10,
  "size": [0.30, 3.5, 91.85], "solid": true, "occludes": false }
```

* The peg's own mesh is a 6 cm cube; `y: -0.10` buries it below the local floor,
  so the drawn geometry is never visible and never blocks the sky.
* `solid: true` with the authored `size` is the whole collider: a 3.5 m tall box
  from −0.10, so it has no ground gap, is far above the 1.0 m jump apex (it can
  never be stepped on) and blocks the player, the AI and the navigation bake
  alike.
* **`occludes: false` is mandatory**: it keeps the box out of the occlusion
  bake and the vertex solve, and the buried mesh keeps every drawn triangle out
  of any receiver's line of sight. The honest test is a measurement: on the
  shipped level, deleting the four pegs moves 0.1475 % of the atlas texels by at
  most 0.18 in the solver's stored radiance scale (mean 0.0011), while deleting
  four ordinary grass tufts moves 0.1660 % with a worst texel of 0.28 — the pegs
  sit at or below the pipeline's own order noise and cast no readable shadow.
  Replacing a boundary with a `wall` would block the lamps, the sky and the
  reflection probes outright.
* The `geometry_intent` annotations for the missing-wall and room-leak
  heuristics name these boundaries, so `places --check-geometry` stays clean
  without a visible wall.

### 35.3 The night look

* `"sky": { "texture": "outdoor:tex_sky_stars_01", "brightness": 1.0, "ambient": 0.0 }`.
  The ambient term stays **0.0**: there is no moon, no sky fill and no exposure
  trick in this level, and the interiors are untouched by it.
* Twelve `outdoor:lamp_stand` props at 7.5 m spacing alternate sides of the
  gravel route, each with the kit's documented `point` light (offset
  `[0, 0.86, 0]`, intensity 0.7, range 7.0, `smooth`). The pools overlap at
  their edges and stay local.
* Seven `outdoor:streetlight` props on ``~12 m`` spacing (six 12 m gaps and a
  10 m one before the connector), alternating sides at x = 2.3/6.7, arms turned
  over the route. Each authors its documented
  downward light at offset `[0, 6.05, 0.43]` (`[1.0, 0.84, 0.66]`, intensity
  1.5, range 14.0, `smooth`), so the tall heads and the real emitters line up
  and the hood keeps the pool on the ground.
* One `fog_regions[]` low mist (`night_low_mist`, x −0.15..24.15, z −90.5..−2.0,
  density 0.055, ground layer −0.20..0.50 with a 9 m falloff) reads as
  restrained ground haze over the yard and ends 1.2 m short of the destination
  house's front wall, which stays dry.
* The boundary-wall tree rows mix the leafy original, the birch and the
  evergreen with deliberate repeats and scale variation, all outside the
  route, connector, doorway and house keep-outs; the four original containment
  pegs are unchanged. Trees are ordinary props: three shared models, three
  shared embedded atlases and one draw per model per spatial cell (the prop
  batch render-instances every repeat), with `occludes: false` keeping the
  coarse canopy boxes out of the bake.
* Both doorways hang two `outdoor:lamp_wall` props on the kit's documented
  mounts (`[±1.15, 2.55, 0.20]`, `y = 2.03`); the destination's flood the
  centred door, the source's light the way back in.
* Both door leaves start **open**, so the interiors' own light spills through a
  real opening (and the return route is never sealed). The destination entry
  room adds one dim `home:ceiling_light_round` at brightness 0.9 so the finished
  interior reads and the doorway has light to spill.
* Trees and grass are `occludes: false` deliberately: the bake's coarse boxes
  would turn a leaf canopy into a solid blob shadow, and alpha-cutout blades
  must never bake as solid rectangles (§34.2, §34.5).

### 35.4 The encounters

| Encounter | Placement | Behaviour |
| --- | --- | --- |
| Carved pumpkin | On the walkway, x = 13.5, z = −89..−83 | `move_to` route at 0.5 m/s with `laugh` pauses; the hop arc is the clip. Attached `flame` glow (1.0/0.52/0.16, 0.7, range 4.5) travels with it. |
| Three sheet ghosts | West grass band, x 0.5..1.6, z ≈ −18/−45/−70 | Independent routes and the shared proximity fade for a 1.6 m figure (near 3.0 m, far 7.0 m, out 1.6 s, in 2.4 s, opacity 0.0..0.85) with a fade-coupled cyan `body` glow. |
| Pumpkin-head skeleton | Middle grass band at (9.6, −52.0), between the grove trees and clear of the path-lamp posts | `nav_agent` + `ai` wanderer (0.571 m/s, radius 2.0, 2 s idle) on the compiled navigation, head-socket orange glow. |
| Ghost cat | Hovers beside the concrete walkway (x 14.2..15.6, z −44..−52) at 0.2 m/s | The shared `fade` proximity form (`near_radius` 2.2, `far_radius` 5.5, out 1.2 s, in 1.8 s) and a cyan `body` glow; it fades out as the player comes close and returns from 5.5 m. |
| Three house guards | Inside the destination house at (10.6, −95.6), (13.5, −95.7), (16.2, −94.6), facing the door | Each authors an `animation` component resting on `collapse_reassemble`; the `night_guard_zone` volume just inside the doorway starts `night_guard_wake` once, whose one step starts all three clips together. The binding needs the sequence idle, so a re-entry during the 12 s clip does nothing, and the edge re-arms after exit and completion. The pumpkin heads carry their own emissive material (no sixth dynamic light). |

All of these use the shared components of §29; none of them is special-cased in
engine code. The demo's dynamic-light budget is 8, and these six glow lights
(one pumpkin, three ghosts, one skeleton, one ghost cat) leave two spare.

## 36. Winter static snow kit

The maintained Winter source is `assets/levels/winter.json`, generated by
`tools/levels/build_winter.py`. Its 28 reusable snow props and mounting table
are documented in `assets/environment/winter/README.md` and
`docs/ASSET_SPECIFICATION.md` §8.7. Use the ordinary prop exporter, material
resolver, lighting bake and map compiler; this kit adds no runtime geometry
or image generation, weather or movement features.

The October 8 reconstruction adds seven village static families in
`tools/props/parts/winter_village.py`: snow-capped coursed walls/piers, a timber
lantern, open masonry entrance frame, braced door hood, deep window frame and
ice fragments. Their explicit scene colliders cover only solid stone/post
bodies; ice fragments and front dressings are non-solid. The frame must leave
the existing 1.14 × 2.15 m doorway open. Use the hood's exposed projecting nose
for snow, keeping its sheltered back bare. Stone/wood surface variants are
Winter-local; the completed Outdoors bases and files remain unchanged.

Preserve `outdoor:tree_03` as the canonical evergreen. The two winter GLBs
retain all base records and add a separate snow mesh; do not tint the tree
white or refit the base. Preserve original tree/rock/rail collision sizes.
Use non-solid cap additions on existing architecture and `guardrails[]`.
Prop mounting Y is floor-relative, so subtract the local rendered floor from
the desired support height, including decks and stair treads. Snow caps sink
8 mm into supports; icicle root planes sit 15 mm inside undersides. Clear
door swings, walking lines, ramps and pond approaches.

Run the snow asset tests and seam check, rebuild `winter.placesmap`, verify it
with `--require-current`, then capture native High/Medium/Low views and the
Winter traversal scripts. The static snow validation report records geometry,
collision, native views and draw cost; do not treat a software preview as a
substitute for the compiled native load.

The integrated scene uses selective exposed accumulation: the sealed lodge
awning shelters its porch rails and window sills. Keep those supports bare.
Pond corner drifts sit on tapered dry shelves, and forest scatter leaves a
clear packed walking spine and a small illuminated resting place. The generator
also synchronizes the severe review's composition while preserving its existing
weather. See `docs/reports/winter-integration.md` for native evidence and limits.

### Winter string-light modules

`winter:string_lights_short`, `winter:string_lights_medium` and
`winter:string_lights_long` are ordinary non-solid emissive GLB props. Their
attachment-centre spans are 2.8, 4.0 and 6.6 m, containing 3, 5 and 7 bulbs.
Use `tools/props/string_lights.py` for the exact shared local attachment and
bulb coordinates; place the base at `attachment_world_y - attachment_height`,
then subtract the local floor to obtain prop `y`. Repeat modules at supports,
keeping their native scale to preserve bulb size. Yaw rotates both the wire
and its prop-owned lights through the existing placement transform.

Author the module's `lights` array from the shared helper: explicit `point`
sources outside the glass, amber `[1,.60,.20]`, smooth finite range. Winter
uses .32 intensity/4.5 m range overhead and .10/3 m at rails. `occludes:false`
avoids coarse cable boxes in the vertex fallback; prepared transport still
sees the actual opaque triangles. All sources bake into the normal package
variants and illuminate architectural and static-model receivers. There are
no runtime light updates or extra lightmap layers.

## Known Implementation Caveats

These are current, documented limitations that affect map
authoring. They are not invitations to change the engine as part of an authoring task.

1. **Quality settings apply live.** Graphics changes rebuild affected GPU resources and use the lightmap cache or background bake where needed. `PLACES_QUALITY` selects a profile for one run.
2. **Emission reaches surfaces and fixture faces, not decals.** A decal is drawn by
   its own pass, which has no emission term; an `emissive` material used as a
   *decal sheet* will not glow. Emission on wall/floor/ceiling materials and on GLB
   prop materials works.
3. **A light's range normalises its falloff.** For a wall sconce or a prop light, `range` is the distance at which the radial pool reaches zero *and* the span the curve is evaluated over, so halving a range makes the pool both tighter and dimmer near the source. For a ceiling fixture, `range` is the horizontal reach of the directional pool: directly beneath the emitter the pool is at full strength whatever the ceiling height, and it falls to zero at `range` measured sideways from the emitting rectangle. There is no separate "cutoff only" mode.
4. **Cone/spot lights are not implemented.** The generic model has point, rectangle
   and line shapes; a directional light needs a response model that does not exist.
5. **A GLB may embed larger prop textures than the shipped native size.** The
   engine accepts up to 1024 px per edge but High and Medium upload a prop sheet
   at 256 and Low at 128, so the shipped toolkit stays at the 256 native size;
   authoring bigger embedded art gains nothing under High unless the engine's
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
10. **Spawn outside every room is accepted** with an initial floor reference of `0.0`, then falls without support. Check it.
11. **`floor_patches` are dimension-unvalidated.** They are capped at 8000 entries,
    but a malformed patch is skipped at build time rather than rejected. Keep them
    well-formed and inside a room.
12. **`pack:` material ids are not resolvable.** No current workflow produces a
    pack `materials.json`, so a level cannot define materials beyond the catalog;
    ship custom artwork through `assets/catalog.json`.
13. **A prop light's `shape` does not infer.** Omitting `shape` makes the light a
    point and ignores `half_width`/`half_depth`/`length`. `tools/assets/validate.py`
    currently infers a shape from those fields, so a level can pass the tool and
    still bake as a point; author `shape` explicitly.
14. **Beyond `doors[]`, dynamic objects are engine-created.** A level authors its
    moving leaves as doors; the washer-drum demonstration is spawned by placing
    `core:washing_machine` and cannot be placed or driven directly.
15. **Documentation drift in shipped sources** (recorded here so agents trust the
    code): `src/level.rs`'s `y` comment says a ceiling fixture's `y` is ignored,
    but the bake honours it for storey selection.
16. **Tooling vs runtime strictness.** The Rust runtime is permissive (unknown
    class/type, missing `model`, invalid `size`, unknown fixture/prop ids degrade);
    `tools/assets/validate.py` is strict and fails. Pass the tool, not the runtime
    fallback.
17. **`props/build.py --check` enforces container validity and decoded texture
    memory but not the triangle/scale/origin art budgets**; those live in
    `cargo test`. Do not treat a clean `--check` as complete budget approval.
18. **Water is a volume, lighting is baked.** "Flooded" needs a `water[]` volume
    over real recessed geometry (a volume over a flat floor reads as a puddle, not
    a pool); "mood lighting" must be expressed with existing materials, geometry and
    per-fixture colour/brightness. Static transport is baked; bounded live
    entity direct/glow response does not rebake the room.
19. **Reflections are per-material and limited.** One planar plane per frame, at most
    two probes per level, probes are static (no realtime update), and a planar material
    reused on non-planar geometry is skipped with a warning.
20. **Audio has no device backend.** There is no audio subsystem in this tree: the `audio` component and `play_sound`/`stop_sound` are implemented as typed emitter state that the frame loop reports once, but nothing is audible, and `play_audio` no longer exists as a tag. `play_animation` and `toggle_animation` dispatch real per-instance animation through the entity runtime, and their target must carry an `animation` component.
21. **An interactable's aimable bound uses the same size contract as collision.**
    It is the level `size` (or `[0.6, 0.9, 0.6]`), scaled — never the catalog size.
    A small prop with no authored `size` is aimable as a standard box, so a map that
    needs a precise aim target authors `size`. A manually interactable door's aim
    bound follows its live collider as the leaf swings.
22. **A void wall box is a whole volume, not a per-face surface.** `solid: true`
    (default) collides as the entire box and `occludes: true` (default) adds the
    entire box to the bake's fast occluder set, exactly like a solid prop's `size`
    box; a box that *encloses* a walkable space is therefore a solid block, so build
    a hollow enclosure from thin slabs or set the flags false. `occludes: false`
    removes the box from the fast occluder set only: the prepared (Medium/High
    lightmap) transport solve still blocks on every drawn opaque face, exactly as it
    does for a prop with `occludes: false`.

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

## Add an interactive door

1. Author the wall opening first: a `kind: "door"` opening sized for the leaf
   (see above).
2. Place the leaf by its **hinge** (see [§30](#30-doors-switches-and-effects)):
   `x`/`z` at the hinge jamb, `y` above the local walkable floor,
   `rotation_degrees` aiming the closed leaf (`0` = +X, `90` = −Z).
3. Give it a unique `id`; give it an `interactable` component (with
   `prompt`/`reach` if the defaults do not fit) and its own `interact` binding.
4. Check the swing clears the room: `open_direction` and `swing_degrees` decide
   which side the leaf moves to.

```json
{ "id": "office_door", "x": 2.0, "y": 0.0, "z": 0.15,
  "rotation_degrees": 0.0, "width": 0.9, "height": 2.1,
  "open_direction": "left", "swing_degrees": 90.0,
  "components": [ { "component": "interactable", "prompt": "Office door" } ],
  "bindings": [ { "on": "interact", "actions": [{ "action": "toggle" }] } ] }
```

```json
{ "kind": "door", "offset": 1.55, "width": 0.9, "height": 2.1, "sill": 0.0 }
```

For a sauna leaf, add `"kind": "sauna"`; for an externally controlled door,
author no `interactable` component and no binding, and drive it with
`open`/`close`/`toggle`/`lock` actions from a switch or a trigger volume.

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
   turns), and `Low` drops planar reflections entirely (Medium and High keep
   them).

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

A tube that reads bright but casts a restrained local pool (the demo's far corridor panel):

```json
{ "fixture": "core:fluorescent_panel_01", "x": 47.5, "z": 13.0,
  "brightness": 0.38, "range": 4.5, "emission": 1.0 }
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

## Emit steam

1. Place the emitter at the source: `x`/`z` in world space, `y` above the local
   walkable floor.
2. Size the plume with `width`/`depth`/`height`; tune the density with `count`
   and `size`.
3. Keep it presentational: an effect never blocks, hides or lights anything (see
   [§30](#30-doors-switches-and-effects)).

```json
{ "kind": "steam", "x": 31.0, "y": 0.0, "z": 12.6,
  "width": 0.9, "depth": 0.6, "height": 1.5,
  "count": 20, "size": 0.34, "drift": 0.16, "lifetime_seconds": 3.2 }
```

## Add a switch that controls a light

1. Give the fixture a stable `id` and `switchable: true`; without that flag the
   action is a load error and its illumination can never change.
2. Give the switch prop an `interactable` component, and an `animation`
   component when its model has a lever clip.
3. Wire one `interact` binding: `toggle_animation` for the lever, then `toggle`
   (or `set_light` with an explicit `"on"`) on the fixture.

```json
{ "id": "hall_switch", "model": "home:wall_switch", "x": 60.3, "y": 1.2, "z": 3.2,
  "rotation_degrees": 180.0, "size": [0.18, 0.18, 0.1], "solid": false,
  "components": [
    { "component": "interactable", "prompt": "Hall switch", "reach": 1.6 },
    { "component": "animation", "clip": "toggle", "looped": false, "playing": false }
  ],
  "bindings": [
    { "on": "interact",
      "actions": [{ "action": "toggle_animation", "clip": "toggle" },
                  { "action": "toggle", "target": "hall_light" }] }
  ] }
```

```json
{ "fixture": "core:fluorescent_panel_01", "id": "hall_light",
  "x": 61.0, "z": 3.2, "brightness": 0.6, "switchable": true }
```

## Add a trigger → timer → sequence chain

1. Author a small `volumes[]` entry over the spot the player must cross; its
   `enter_volume` binding is `once: true` when the chain must run a single time.
2. The binding arms the timer: `set_state` (optional, so a `state` condition can
   see the phase) then `start_timer`.
3. Author the `timers[]` entry (headless: no geometry) whose `on: "timer"`
   binding runs `start_sequence` on the entity the sequence should control.
4. Author the `sequences[]` steps; give the sequence's owner a `state` component
   with the name the sequence writes.
5. Keep the chain short: one wait, one state write, one emission is a full,
   readable example (see [§29](#29-entities-components-bindings-volumes-timers-sequences-and-spawns)).

```json
"volumes": [
  { "id": "sauna_warmup_zone", "x": 26.5, "z": 12.7, "width": 1.2, "depth": 1.2,
    "bindings": [
      { "on": "enter_volume", "once": true,
        "actions": [
          { "action": "set_state", "target": "sauna_warmup_timer",
            "name": "phase", "value": "armed" },
          { "action": "start_timer", "target": "sauna_warmup_timer" }
        ] }
    ] }
],
"timers": [
  { "id": "sauna_warmup_timer", "seconds": 1.5, "repeat": false,
    "bindings": [
      { "on": "timer",
        "actions": [{ "action": "start_sequence",
                      "sequence": "sauna_warmup", "target": "sauna_door" }] }
    ] }
],
"sequences": [
  { "id": "sauna_warmup", "steps": [
      { "step": "wait", "seconds": 0.5 },
      { "step": "set_state", "name": "phase", "value": "warm" },
      { "step": "emit", "on": "timer", "key": "warm" }
    ] }
]
```

## Compile a level into a `.placesmap`

1. Author the level JSON as described in this guide, then validate it with the
   loader tests (`python3 -m unittest tests.test_package`) and
   `cargo test --workspace --all-features`.
2. Compile it:
   `./target/release/places-compile build my_level.json`
   The compiler runs the static preparation (lighting bake, geometry, lightmap
   atlas, collision, reflection probe capture) and writes `my_level.placesmap`
   beside the source. Use `--variants off,medium,full` to choose the packaged
   lightmap qualities, `--workers N` to bound CPU use, and `--force` to rebuild
   when the incremental check says the package is current.
3. Verify the result: `./target/release/places-compile validate my_level.placesmap`
   decodes every record and re-hashes every entry;
   `./target/release/places-compile verify my_level.json --package my_level.placesmap --require-current`
   checks final source/catalogue/dependency/compiler currency;
   `./target/release/places-compile inspect my_level.placesmap` prints the
   manifest and resource list.
4. Drop the `.placesmap` into `levels/` (or use the in-game Import action) and
   boot it with `PLACES_LEVEL=<id>`. Editing the source again leaves the package
   stale; recompile rather than editing records by hand.

`./target/release/places-compile build-collection levels/` compiles every source
in a directory, reporting per-source failures without blocking the others.

Current package major is 1, level schema 3, geometry revision 9 and transport
solver revision 17. PLMP v6 shares exact vertex/frame bytes without changing
decoded material response, normals, UVs, lighting or character caster claims;
literal v3/v4/v5 remain readable with their documented defaults. Both encoded
and expanded literal prop records retain the 512-MiB bound. PLPF v3 preserves spatial selected
direct presence even with zero sources; v2 remains a genuine legacy centre path.
Probe positions v3 declares HDR captures; v2's old RGBA8 data decodes to linear
without recovering already clipped energy. These reader guarantees do not make
historical prepared fingerprints current.

Ordinary builds include placed models and every runtime spawn template in the
dependency closure, including unused override choices. GLB SHA covers embedded
PNG bytes; editing a standalone model-source PNG requires exporting its GLB first.
`build-inputs.json` revision 1 binds source, catalogue, executable and capture mode
alongside format/geometry/solver/variant identity. Missing or corrupt provenance
requires rebuilding, and `--force` bypasses both package and prepared-product reuse.
Metadata, navigation/AI and final presentation edits may reuse prepared products;
physical material, image, model, geometry, source and entity changes rebuild
conservatively. Source/assets must stay stable through each compiler invocation.

At package-open the player streams external dependency SHA-256 once and checks
new packages against the installed catalogue. Same-size substitutions are named
rebuild errors. Genuine old packages without provenance retain their compatible
immutable-bundle contract. The runtime cache keys prepared manifest identity and
quality; no file hashing, source compilation or static bake occurs per frame.

## Beach construction pieces

The `beach:` catalog provides 27 static visuals, three animated animals, ten
surface PNG families, two sea materials and a seamless day panorama. The
[Beach kit](../assets/environment/beach/README.md) maps B01–B18 to resource IDs
and gives deterministic source locations. The connected
`assets/levels/beach_demo.json` is generated by `tools/levels/build_beach.py`.

`tools/levels/beach_components.py` supplies `shore_segment(x, sea_z, shore_z,
width)` and `daylight()` as existing v3 arrays. The shore has real lowered
support, two gentle ramps and separate deep/shallow WaterDef volumes. Ramps
are narrow enough that their longer dimension runs north/south, as the current
engine requires. The containing room begins at the seabed so underwater entity
anchors receive prepared lighting, while raised dry support reaches world Y=0.
Water X edges are inset 2 cm to clear the 1 cm room-edge tolerance, keeping
corners inside their own room when
adjacent strips have different shore positions. Inset water within separately
lowered room basins too: exact shared edges resolve to the earlier room.
Place adjoining strips to follow a curved coast; use the
scalloped foam GLB along their visual edge. Decorative terrain never substitutes
for a floor. Keep raised deck regions outside ramps to preserve one support
owner at each point.

The demo retains one continuous 48×52 m seabed room and 64 narrow shore
supports. Its central curved cove joins outer tangent runs capped at 1/12;
1/64 m binary-exact shore positions limit lateral sand risers to 9.375 mm
and underwater risers to 25 mm. Longer foam ribbons are independent scenery.
Do not split this volume into sub-probe-width rooms: a fitting atlas alone
does not prove that each room has valid irradiance probes.

`tools/props/parts/beach_structures.py::placed_components` emits separate
visual, blocker and support arrays for dock, hollow timber buildings, town
portals, terraces and rotated stairs. Inputs are a world base position, the
containing room's `floor_y`, quarter-turn yaw and unique instance identity.
Its optional `floor_at(x,z)` callback resolves pre-existing lowered regions or
ramps; returned prop Y offsets already compensate for the helper's own support.
Merge the arrays into real containing rooms and author safe approaches/landings.
Separate tight pier/curved-header blockers preserve standing headroom through
the visible portals. Twelve tread regions supply
stair traversal for any quarter turn; no unsupported stair direction is used.
Thin companion surfaces sit inside visible stock, avoiding duplicate faces.

`beach_nature.py::placed_components("sea_arch", ...)` supplies separate tight
piers/header blockers around the real opening, with the same world-base and
floor callback contract. Author its supported approach or seabed separately.
Other nature visuals need separate collision where reachable; palm bounds in
`PALM_TRUNKS` measure the bent trunk rather than canopy. Animal READMEs contain
explicit initialization actions and their measured looping pose envelopes.
Perched gulls use Y=0 relative to actual deck support; fish use a positive Y
offset above a submerged floor. Flying and swimming stay within their authored
lighting anchors. The day helper matches panorama sun direction; lighting uses
the existing directional, ambient, HDR bake and probe paths.

## Frutiger Aero construction pieces

The `frutiger_aero:` kit provides 23 static modules, six material roles, four
periodic surface families, five fitted decals and a day panorama. Its connected
`assets/levels/frutiger_aero_demo.json` is generated by
`tools/levels/build_frutiger_aero.py`: glazed domed atrium, branded corridor,
walk-through tube, reception and looping garden terrace. The
[Aero kit](../assets/environment/frutiger_aero/README.md) maps all fifteen
reference groups to actual resources and records their UV contracts.

`tools/props/parts/frutiger_aero.py::placed_components` accepts world X/Z,
base Y, quarter-turn yaw and unique identity. It returns a non-solid visual
plus tight separate structural stock. Preserve open arch/door/tube apertures;
never replace them with their full bounding box. The basin uses a real circular
WaterDef and inscribed collider strips whose .095 m top lies inside the visible
.10 m floor slab, rather than rendered rectangular floor regions. Existing
daylight, prepared lightmaps/probes, direct lights, gloss, BLEND and water paths
remain authoritative; the kit requires no creature or locomotion animation.

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

The prop toolkit accepts `alpha_mode="blend", opacity=0.28` on a named
material slot. Opacity is a finite 0..1 glTF base-colour alpha multiplier;
retain the model atlas UVs and PNG alpha. Non-BLEND slots require opacity 1.
This uses existing translucent rendering and transport, including centre
sorting limits; it does not add refraction or nested-glass guarantees.

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

## Add an invisible containment collider

1. Decide the box the player must not cross: `[width, height, depth]` metres,
   centred on `(x, z)`, with its base at the local floor plus the prop's `y`.
2. Place one `outdoor:collision_peg` per box:

   ```json
   { "id": "boundary_east", "model": "outdoor:collision_peg",
     "x": 24.30, "z": -45.925, "y": -0.10,
     "size": [0.30, 3.5, 91.85], "solid": true, "occludes": false,
     "comment": "Invisible containment: the peg is buried under the floor." }
   ```

3. Sink it: `y` at or below minus the peg's own 0.06 m height keeps the drawn
   geometry under the floor, so nothing is visible and nothing can be lit.
   `occludes: false` keeps the box out of the baked lighting; the buried mesh
   keeps it out of the prepared solve's line of sight.
4. Size it so it has **no ground gap** (start below the floor) and **no
   staircase** (the top must be well above the jump apex, 1.0 m, and above the
   0.4 m step). A base at −0.10 with height 3.5 m satisfies both.
5. It blocks the player, AI movers and the navigation bake exactly like a wall
   slice, so no separate navigation authoring is needed. Never seal a door with
   one: check every doorway's approach after placing.
6. Annotate the perimeter for the geometry checker: add a `geometry_intent`
   rectangle (`check: "missing-wall"` / `"room-leak"`) covering each finding the
   open perimeter produces, with a `note` saying the boundary is invisible.
7. Verify: `places --check-geometry --level <map>` reports 0 errors, then walk
   and jump into the boundary in the running game and confirm the player never
   leaves the authored area and the feet never land on the box's top.

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
* opening types or opening behavior, including `glass` and `solid`;
* door kinds, door fields, door actions, switchable fixtures, or the
  locked/externally-controlled contract;
* component kinds or fields, event kinds, condition checks, action tags or
  their legal targets;
* trigger volume, timer, sequence or spawn fields and their limits;
* effect kinds or effect fields;
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
