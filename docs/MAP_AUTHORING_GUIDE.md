# Places Map Authoring Guide

| Field | Value |
| --- | --- |
| Document status | **Canonical / living.** Update it whenever the authoring contract changes (see [Maintaining This Guide](#maintaining-this-guide)). |
| Level format version documented | `1` (`format_version` in every level JSON) |
| Asset catalog format version documented | `2` (`format_version` in `assets/catalog.json`) |
| Last verified commit SHA | `08115a5` (Batch 2: baked lightmaps, static prop occlusion, dynamic-object path) |
| Verification performed | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace --all-features`; `python3 tools/assets/validate.py`; `python3 tools/textures/build.py --check`; `python3 tools/props/build.py --check`; `python3 -m unittest tests.test_package`; `cd level-editor && npm test` |
| Primary benchmark level | `assets/levels/places_demo.json` |

> This revision documents the Batch 1 foundation (Full/Low quality profiles, the
> generic engine-level light model, true emissive materials) **plus the Batch 2
> lighting work**: baked lightmaps for static world geometry with the vertex-lit
> path as an exact fallback, automatic static-prop occlusion, and a separate
> dynamic-object render path. Read
> [Known Implementation Caveats](#known-implementation-caveats) before relying on
> engine limits, and re-run the validation commands after pulling new commits.

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
marks it **PLANNED — not authorable** and never shows planned syntax as usable.

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
   (movement and collision), `src/render/` (meshes, decals, fixtures, props),
   `src/lighting/` (bake), `src/materials/` + `src/assets.rs` (catalog and materials).
2. **Validation and tests** — `src/loader/tests.rs`, `src/level/tests.rs`,
   `src/render/tests.rs`, `src/materials/tests.rs`, `src/assets/tests.rs`,
   `src/props/tests.rs`, `src/collision/tests.rs`, `src/game/tests.rs`,
   `src/lighting/tests.rs`, and the audit modules listed in
   [Validation Workflow](#27-validation-workflow). Tests pin the accepted contract.
3. **Asset catalog** — `assets/catalog.json`, plus `assets/README.md`.
4. **Known-good shipped content** — `assets/levels/places_demo.json`,
   `tests/fixtures/levels/*.json`.
5. **This guide** — update it when 1–4 change.
6. **Design documents** (e.g. `Places-resolved-design-decisions.md`) — aspirational
   only. They describe intent, not a contract, and must never be quoted as syntax.

Authoritative paths:

| What | Path |
| --- | --- |
| Level schema | `src/level.rs` |
| Loader / validator | `src/loader.rs` |
| Collision | `src/collision.rs`, `src/game.rs` |
| Mesh generation | `src/render.rs`, `src/render/geometry.rs` |
| Decals | `src/render/decals.rs`, `src/render.rs` |
| Fixture geometry | `src/render/fixtures.rs`, `src/lighting/tuning.rs` |
| Prop loading | `src/props.rs`, `src/gltf.rs`, `src/render/props.rs` |
| Lighting bake | `src/lighting/`, `src/lighting/tuning.rs` |
| Materials / textures | `src/materials/`, `src/assets.rs` |
| Quality profiles / downscaling | `src/quality.rs` |
| Emissive material model | `src/materials/emission.rs` |
| Generic light model | `src/lighting/light.rs` |
| Catalog | `assets/catalog.json` |
| Benchmark level | `assets/levels/places_demo.json` |
| Regression fixtures | `tests/fixtures/levels/` |

### Quick implemented-vs-planned reference

| Capability | Status |
| --- | --- |
| Rectangular rooms, per-room `floor_y`, `height`, flat/gable ceiling | Implemented |
| Rectangular walls, per-face materials, doors/windows/passages/vents | Implemented |
| `floor_patches` (material-only), `floor_regions` (recess/raise), 0.4 m step rule | Implemented |
| Baked lightmaps for static world geometry (floors, ceilings, walls, reveals, skirts), with the baked-vertex path as the exact fallback | Implemented |
| Baked vertex lighting, 3 fixture families, per-light colour/intensity/range/falloff, partitions, vertical isolation | Implemented |
| Static props occlude baked light (contact darkening, blocked pools), derived from the placed model's own triangles | Implemented |
| A separate dynamic-object render path (per-frame transforms, no rebuild of static geometry or lightmaps) | Implemented (one demonstration object in Places Demo; not authorable from a level yet) |
| Generic engine-level lights (point / rect / line) owned by fixtures and props | Implemented |
| Material emission (`emissive`, `emissive_intensity`, `emissive_mask`) and per-fixture `emission` | Implemented |
| Full / Low runtime quality profiles with texture downscaling | Implemented |
| Props/entities from GLBs by logical id, `solid` collision boxes | Implemented |
| Multi-primitive / multi-material GLB props, embedded emissive materials, node transforms | Implemented |
| External PNG surfaces, decals, fixture faces; catalog + themes | Implemented |
| Level `.zip` packs with `materials.json` and pack textures | Implemented |
| Water, transparency/glass, realtime dynamic lights, realtime shadow maps, animation/skinning | Not implemented |
| Emissive decals; per-placement emission overrides; cone/spot lights | Not implemented |
| Ramps/sloped floor regions; ceiling/floor openings; traversal between stacked storeys | Not implemented |
| Room-wide brightness/tint modifiers; non-fixture decor meshes beyond props | Not implemented |
| WebP or formats other than PNG; arbitrary structural meshes | Not implemented |
| `lights` / `wall_lights` level arrays; per-room wall material | Not implemented (wall fixtures live in `ceiling_lights` with `"mount": "wall"`; prop-owned lights live in `props[].lights`) |

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
11. **Add props.** Logical ids, x/y/z, rotation, `size` for solid props. Mind +Z fronts.
12. **Add decals.** One flat surface each; never across height changes; no gable ceilings.
13. **Add lights.** Choose fixture types per intended look; place enough fixtures to
    light the route; use colour/intensity for mood. Wall fixtures need `mount`+`y`.
14. **Check collision.** `solid` props, door headers, window sills, unwalkable rims.
    Walk the route in-game mentally against the 0.4 m step rule.
15. **Inspect for overlapping/coplanar geometry.** No duplicated floors or walls;
    doorway thresholds owned once; no wall ends buried in other walls.
16. **Validate assets.** Run the catalog, texture and prop checks
    ([Validation Workflow](#27-validation-workflow)).
17. **Run tests and boot the level.** `cargo test --workspace`, then
    `LIMINAL_LEVEL=<id> cargo run` and read the console. A level that fails validation
    is silently absent from the menu, so always boot it explicitly at least once.
18. **Visual/render validation if available.** Screenshot with
    `LIMINAL_CAPTURE=frame.png` and inspect: no holes, no flicker, no light leaks,
    no floating props.

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
`"color": [0.35, 1.0, 0.45]` and low `brightness` → validate.

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

---

## 5. Level File Structure

A complete level is one JSON object. This is the full current skeleton — every field
below exists in `src/level.rs`; nothing else in a level document has any effect.
Unknown keys are **silently ignored** (no `deny_unknown_fields`), so typos fail without
an error; diff against this skeleton.

```jsonc
{
  "format_version": 1,                 // REQUIRED. Must be exactly 1.
  "id": "my_level",                    // REQUIRED. Non-empty; menu key.
  "name": "My Level",                  // REQUIRED. Non-empty; display name.
  "author": "",                        // optional, default "".
  "spawn": { "x": 2.0, "z": 5.0, "yaw_degrees": 0.0 },   // REQUIRED (x,z required)

  "defaults": {                        // optional; see the warning below
    "wall": "core:wallpaper_yellow_01",
    "floor": "core:carpet_beige_01",
    "ceiling": "core:ceiling_panel_01"
  },

  "rooms": [                           // floors + ceilings only; no implicit walls
    { "x": 0.0, "z": 0.0, "width": 9.0, "depth": 7.0, "height": 2.7 }
  ],

  "walls": [
    { "x": 0.0, "z": 0.0, "width": 9.0, "depth": 0.3,
      "openings": [ { "kind": "door", "offset": 2.0, "width": 1.2,
                      "height": 2.1, "sill": 0.0 } ] }
  ],

  "floor_patches": [                   // material-only overlays (no elevation)
    { "x": 3.0, "z": 5.0, "width": 2.0, "depth": 1.5,
      "material": "core:carpet_damp_01" }
  ],

  "floor_regions": [                   // recesses / raised platforms
    { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 3.0,
      "offset_y": -1.5,
      "material": "core:pool_tile_basin_01",
      "edge_material": "core:pool_tile_wall_01" }
  ],

  "decals": [
    { "x": 4.5, "z": 2.0, "width": 0.9, "height": 0.9,
      "material": "core:decal_no_diving_01", "surface": "floor" }
  ],

  "ceiling_lights": [                  // ALL fixtures, ceiling- and wall-mounted
    { "fixture": "core:fluorescent_panel_01", "x": 4.5, "z": 3.5,
      "brightness": 0.7, "color": [1.0, 0.94, 0.82] }
  ],

  "props": [
    { "model": "core:desk", "x": 2.0, "z": 5.0, "rotation_degrees": 0.0,
      "solid": true, "size": [1.6, 0.75, 0.7] }
  ]
}
```

Two collections are merged/legacy and should not be used in new maps except for
compatibility: `room` (a single `RoomDef`, appended **after** `rooms`) and the catalog's
legacy `props` array (see [Asset Catalog](#14-asset-catalog)).

**Warning — the `defaults` gotcha.** If the `defaults` key is absent entirely, the
engine uses `core:wallpaper_yellow_01` / `core:carpet_beige_01` /
`core:ceiling_panel_01`. If you author `defaults`, author **all three keys**: an empty
string material id is accepted and resolves to the *untextured* surface, not the
default. Never leave a material id empty.

**Where levels live.** `assets/levels/*.json` ships with the game;
`levels/*.json` and `levels/*.zip` are drop-in packs. Both appear in the Level Select
menu. `tests/fixtures/levels/` is for engine regression fixtures and is never packaged.
A level that fails validation is skipped without a console line at discovery — boot it
with `LIMINAL_LEVEL=<id>` to see the error.

### Level limits (validation)

| Limit | Value |
| --- | --- |
| Rooms | ≤ 500 |
| Walls | ≤ 5000 |
| Ceiling lights | ≤ 5000 |
| Props | ≤ 5000 |
| Decals | ≤ 5000; each edge ≤ 10 m |
| Floor regions | ≤ 2000 |
| Room dimensions | width/depth ≤ 2000 m, height ≤ 50 m |
| Estimated floor area | ≤ 1 000 000 m² |
| Estimated generated vertices | ≤ 2 000 000 |
| Distinct prop models placed | ≤ 256 |
| Summed baked prop vertices | ≤ 1 500 000 |

These are enforced by `src/loader.rs` at load time with named rejection messages.

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
| `spawn.yaw_degrees` | number | no | `0.0` | 0 = north (−Z), +90 = east (+X). |

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
  ceiling `core:ceiling_panel_01`.

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
| `ceiling_material` | string | no | `defaults.ceiling` | Ceiling material override for this room. |

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

* Tagged enum: `kind` is `"flat"` or `"gable"`.
* `ridge` is the axis the ridge runs **along**: `"x"` leaves the ridge constant in X
  and slopes the ceiling along Z; `"z"` is the mirror case. The ridge sits at the
  footprint midpoint of the perpendicular axis.
* `ridge_rise` is metres above the eave, `> 0`, `≤ 50`.
* Gable ceilings are real sloped geometry (two slopes with a ridge cut line). A wall
  whose `height` is omitted follows the local ceiling, including splitting at a
  crossing ridge; a wall with an authored `height` is rigid and can poke through.
* Gable-end walls follow the slope unless they author their own height.
* **No decals on gable ceilings** and no ceiling openings.

### Room overlap and ownership

Overlapping rooms are legal and sometimes intentional (they are how stacked storeys
and vertical features are built). Two different ownership rules apply:

* **Geometry, collision and the walkable floor** use the **first room in `rooms`
  order** whose footprint contains the point (0.01 m tolerance). If several rooms
  overlap, the earlier one wins.
* **Baked lighting** uses the **smallest-area** room at that point, with a height hint
  when the light authors a world `y`.

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
| `faces` | object | no | `{}` | Per-face material overrides; wins over `material`. |
| `openings` | array | no | `[]` | Rectangular cutouts; see [Openings](#9-openings). |

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
ceiling or across rooms.

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

### Avoiding duplicate coplanar surfaces

This is the single most common geometry failure. Two walls that share a plane and
overlap in length **and** height physically duplicate a surface; the renderer
coalesces them, the last covering member's material wins along each
(length × height) cell, and end caps buried inside another wall are clipped. That
prevents most z-fighting but it is not a licence to duplicate:

* Never place two walls with the same footprint or the same length-face plane
  "because it renders the same". Overlap makes materials ambiguous and can surface
  as flicker at angles.
* Never continue a wall by starting a second wall at the same base/height over a
  different length without reason; the renderer will merge them, but authored
  overlap is fragile.
* Wall end caps/reveals are clipped against abutting walls; a wall end buried in
  another wall contributes nothing visible.
* The surface audit (`cargo test surface_audit`) only checks Places Demo and fixed
  cases. There is no general z-fighting detector for your level; inspect junctions
  manually and keep one physical wall per surface.

---

## 9. Openings

An opening is a rectangular hole cut through a wall's thickness. All opening kinds
are the same rectangle; `kind` only changes labels and one lighting behavior.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `kind` | string | no | `"door"` | Free string; documented spellings `door`, `window`, `passage`, `vent`. Unknown strings load (forward compatibility). |
| `offset` | number | **yes** | — | Distance along the wall's length axis from the min corner to the opening's near edge; `≥ 0`. |
| `width` | number | **yes** | — | Cut width along the wall; `> 0`; `offset + width ≤ length` (tolerance 1e-3) or the level is rejected. |
| `height` | number | **yes** | — | Cut height above the sill; `> 0`. |
| `sill` | number | no | `0.0` | Bottom edge above the wall's base (`wall.y`); `≥ 0`. `0.0` reaches the floor. |

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
bounded **doorway baseline light blend** between the connected rooms, but only when
the opening's bottom reaches the lower of the two connected floors.

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
  that reach the lower floor blend baselines.
* **Openings are clamped to the wall and the local ceiling.** An opening larger than
  the wall can remove it entirely; an opening whose vertical span misses the wall
  entirely is accepted by validation and silently does nothing.
* **Rejections** name the problem: `Wall {i} opening {j} starts before the wall`,
  `… cannot have a negative sill height`, `… must have a positive width and height`,
  and `Door/Window opening extends beyond this wall (wall {i}: opening ends at {x} m,
  wall is {y} m long)`.
* **Doorway thresholds**: adjacent rooms meeting at a doorway should have their
  floors meet at the wall's centre plane. The renderer subtracts floor coverage from
  wall caps so the two floors jointly cover the threshold exactly once. Do **not**
  author a sill-top surface that is coplanar with a floor; express a raised
  threshold as a floor region instead. This was a real z-fighting bug
  (commit `1783828`).
* **Windows/vents do not connect baselines**, so a room lit only through a window
  stays at its own baseline + the pool that physically passes through the aperture.

---

## 10. Floors, Elevation and Vertical Geometry

### `floor_patches` — material-only overlays

```json
{ "x": 3.0, "z": 5.0, "width": 2.0, "depth": 1.5, "material": "core:carpet_damp_01" }
```

All five fields are required. A patch changes the floor material of an area with no
elevation change. Later patches win over earlier ones, and a floor region's own
material wins over patches. **Patches are not validated** (no dimension/count checks,
no room-overlap requirement); malformed values are skipped at build time. Keep them
inside a room and well-formed.

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
| `material` | string | no | room floor material | Region floor material. |
| `edge_material` | string | no | `defaults.wall` | Vertical transition (skirt) material. |

Rules that matter:

* Regions are **not scoped to a room**: the same rectangle applies to every room it
  overlaps, resolved against that room's `floor_y`. The region's surface must stay
  below each overlapping room's eave or the level is rejected
  (`Floor region {i} sits at or above the ceiling of room {r}`), and a region that
  overlaps no room is rejected (`Floor region {i} lies outside every room section`).
* **Last authored region wins** per point, like patches. Overlapping regions are
  legal; they create their own skirts at their edges.
* Regions generate real vertical transition faces (skirts). Always give recesses an
  `edge_material` — a missing one falls back to the wall default and can look like
  unfinished space (this hid a real bug: a pool-basin wall once rendered as office
  wallpaper).
* `offset_y: 0` is a legal region that only changes material (like a patch) and
  emits no skirt.

### The walkable step rule

**A height change of at most 0.4 m is walked instantly; a larger change is a solid
rim** — solid from the lower side, and refused from the upper side. Staircases are
chains of floor regions whose consecutive offsets differ by ≤ 0.4 m (Places Demo
stair: 1.5 → 1.2 → 0.9 → 0.6 → 0.3 risers). This is what makes pool basins safe
without fall physics.

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
  selects the storey. Unauthored ceiling fixtures use the smallest containing room.

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

Materials are **definitions**, not files. A material entry carries:

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `texture` | **yes** | — | Logical id of a `texture` asset that owns the PNG. |
| `tile_metres` | no | `2.0` | World metres covered by one repeat, both directions. Range `0.05`–`64`. |
| `tint` | no | `[1,1,1]` | Per-channel multiply, each `0.0`–`1.0`. |
| `surface` | no | — | `wall`, `floor` or `ceiling`; documentation/validation only. Geometry decides which family a material draws on; a level may legally use any material on any surface. |
| `emissive` | no | — | `[r, g, b]` (not hex), each `0.0`–`1.0`. Makes the surface read bright on its own; see [Emission](#emission-materials-that-glow). |
| `emissive_intensity` | no | `1.0` when `emissive` is set | Multiplier for the emissive colour, `0.0`–`8.0`. |
| `emissive_mask` | no | — | Logical id of a `texture` asset whose RGB restricts *where* the surface emits. Same rules as `texture` (must exist, file-backed, `.png`). |

Where a level can name a material (all resolved at load):

* `defaults.wall` / `defaults.floor` / `defaults.ceiling`
* `rooms[].material` (floor) and `rooms[].ceiling_material`
* `walls[].material` and `walls[].faces.<face>`
* `floor_patches[].material`
* `floor_regions[].material` and `floor_regions[].edge_material`

The renderer multiplies: **sampled texture × material tint × baked light**, where
*baked light* is the lightmap atlas texel for static world geometry (see
[Lighting](#18-lighting)) and the baked vertex colour on the vertex-lit fallback
path.
There is no gamma handling; the shipped art, tints and lighting constants were
calibrated together in that space. Author with the tint in mind (the office wallpaper
tint, for example, is `[0.85, 0.80, 0.42]`, so the PNG is authored pale).

### Emission: materials that glow

Emission is a **material** property and nothing else:

```text
how bright a surface reads        = material emission   (emissive x emissive_intensity x mask x texture)
how much a room is illuminated    = generic light sources (see section 18)
```

The two are independent by construction. An emissive surface is added *on top of*
the baked lighting, so it stays visibly bright in a dark room, and it never
brightens anything around it: emission does not create light. Author an ordinary
`light` on the object (or a fixture, or a prop-attached light) if the object should
also illuminate the room.

* **No mask**: emission is modulated by the material's own texture, so artwork shapes
  the glow (`emission = emissive x intensity x texture`).
* **With `emissive_mask`**: the mask's RGB also modulates it, restricting the glow to
  the masked region.
* **Old materials**: a material without `emissive` emits nothing and renders exactly
  as it always did.
* **Fixture faces** are emission too: a placed fixture's visible face glows with its
  authored `color` and `emission` (which defaults to its `brightness`) and never takes
  part in the room's baked light.

Failure behavior: an unknown material id or a broken texture logs a
`[materials] {level}: …` line and draws the shared magenta/black diagnostic texture.
A level can load with a visibly broken surface; there is no silent substitution with
unrelated art.

Never write a filesystem path where a logical id is expected. A path in `material`
resolves to nothing and draws the diagnostic pattern.

---

## 12. Textures

**Repository rule: normal editable game textures must exist as real image files in
the asset tree.** Do not generate normal game artwork procedurally from Rust/source
code at runtime. Create the PNG, register it in the catalog, reference it by logical
id. The few remaining code-generated images are internal diagnostics/UI and are
listed at the end of this section — they are exceptions, not the authoring path.

### Supported formats and limits

| Property | Value |
| --- | --- |
| File format | **PNG only.** Signature-checked; RGB, RGBA, grayscale, grayscale+alpha, palette (with/without `tRNS`) all normalise to 8-bit RGBA; 16-bit is stripped to 8-bit. |
| Hard edge limit | **1024 px** on either edge, enforced for every PNG (surfaces, decals, fixtures, pack textures): `texture dimensions {w}x{h} exceed the 1024x1024 limit`. |
| Preferred edge | **256 px** — a soft tooling/policy warning, not a runtime error. |
| Surface sheets | **Square** (both edges equal); otherwise a `tile_metres` cell would stretch. |
| Decal sheets | **Power-of-two on both edges** (mipmapped fitted sampling; POT is the shipped-asset policy). |
| Fixture faces | **Power-of-two on both edges** likewise. |
| Decoded surface budget | ≤ 4 MiB RGBA8 per sheet (one 1024×1024 sheet is exactly 4 MiB). |
| Surface wrapping | `REPEAT` + mipmaps. Surfaces are expected to tile. |
| Decal wrapping | `REPEAT` + mipmaps, but full-sheet fitted UVs, so the sheet never actually repeats. |
| Fixture wrapping | `CLAMP_TO_EDGE` + mipmaps; the whole sheet is fitted once across the face. |
| Alpha | Surfaces are opaque (base pass has blending off). Decals are alpha cut-outs (alpha < 0.5 discarded). Fixture faces are opaque. |
| Colour space | No gamma handling; texture × tint × baked light in display space. |

Shipped Office/Pool surface sheets are intentionally 1024×1024, square, opaque;
`python3 tools/textures/build.py --check` reports them as "over preferred" warnings by
design. The preferred 256 px size is a budget warning, not a rejection.

### Runtime quality profiles and downscaling

The source PNG is *not* what necessarily reaches the GPU. Two quality profiles
(`settings.json` → `"quality": "full" | "low"`, default `full`) decide a **runtime**
edge budget per texture class:

| Texture class | Full (default) | Low |
| --- | --- | --- |
| Surface sheet | 1024 | 256 |
| Fixture face | 1024 | 256 |
| Decal sheet | 1024 | 256 |
| Prop sheet (GLB) | 256 | 128 |
| Emissive mask | 512 | 128 |
| Lightmap atlas page | 1024, 12 texels/m | 512, 8 texels/m |

* **Full is the historical Places runtime size.** Every shipped asset is already at
  or below it, so Full uploads the decoded image unchanged — no rescaling, no visual
  change.
* **Low uses the same assets** and box-filters each image once, at level load, to the
  Low budget. It is not a second art library; ids, materials and geometry are
  identical.
* Downscaling happens once per upload, never per frame, and the result is cached with
  the texture it produced. Full and Low are deterministic: the same source always
  produces the same runtime image.
* The source hard limit (1024 px) is unchanged by either profile: quality only decides
  how much of an accepted source reaches the GPU.
* The **lightmap atlas** is baked light data, not shipped artwork (see
  [Textures](#12-textures)): the same level bakes at the profile's density, so Low
  needs no separate level or hand-authored lightmap set.

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
| Repeating surface texture | Wall/floor/ceiling artwork | Tiles (`REPEAT`) | Opaque | `texture` + a `material` |
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
| Generated decal atlas (256×256; only `core:decal_test_01`) | `src/render/decals.rs` | Internal validation marking; the three external decal sheets are ordinary PNGs. |
| White sheet (2×2) | `src/render.rs` | Untextured geometry (fixture housings, UI quads). |
| HUD font atlas (128×64) | `src/font.rs` | Project-owned bitmap UI font. |
| Lightmap atlas (up to two pages, quality-profile sized) | `src/lighting/lightmap/` | Baked *light data*, derived at level load from the level's own lights and geometry — the texel equivalent of the baked vertex colours it replaces. Not authored artwork, and deliberately not shipped as PNGs: it changes whenever a light, prop or surface moves, and it is regenerated (never re-saved) on load. |

Everything else the renderer draws from an image comes from a PNG under `assets/`.
Every *surface, fixture, decal and prop texture* is still a real PNG asset under
`assets/`, including the lightmaps' albedo partners; the lightmap atlas is the
only thing the renderer samples that is generated at runtime, and it is lighting
data rather than texture artwork.

### Texture budget summary

| Budget | Value | Enforced by |
| --- | --- | --- |
| PNG hard edge | 1024 | runtime decoder + `tools/textures/build.py` + package tests |
| Preferred edge | 256 | tooling warning + shipped-asset policy tests |
| Surface square | both edges equal | policy tests |
| Surface decoded bytes | 4 MiB | policy tests |
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

Two themes ship: **`office`** and **`pool`**, declared in `assets/catalog.json`'s
`themes` array. A theme is an organizational collection with a display name and a
description. Generic/shared assets omit `theme`.

**Themes organize; they never restrict.** No code path rejects an asset because a room
has a different theme, and there is deliberately no theme-filtering query. Place Pool
fixtures in an office, mix themes in one room, or use generic `core` props anywhere.
Rooms have no mandatory theme field.

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
| `display_name` | string | optional | legacy `name`, then the id | all |
| `name` | string | optional | — | legacy alias for `display_name` |
| `asset_class` | string | **required** | — | all. Validated slug; shipped: `environment`, `entity`, `core`, `diagnostic`. |
| `theme` | string | optional | none (generic) | all. Organizational only. |
| `asset_type` | string | **required** | — | all. Shipped: `prop`, `entity`, `material`, `texture`, `light`, `decal`. |
| `source` | `"file"` \| `"definition"` \| `"generated"` | optional | inferred: `texture` → `definition`; else `model` → `file`; else `generated` | all |
| `model` | string | type-specific | — | Resource path **relative to `assets/`**. `.glb` for props/entities; `.png` for file textures, file decals and file lights. Must not be declared by `generated`/`definition` entries. |
| `size` | `[w,h,d]` | optional (validator requires it for placeables) | invalid values dropped; runtime fallback `[0.6, 0.9, 0.6]` | props, entities |
| `color` | `"#rrggbb"` | optional | `#8a8a8a` | props, entities (placeholder box + editor) |
| `category` | string | optional | `"Other"` | props, entities (organizational) |
| `solid` | boolean | optional | `false` | props, entities (catalog advisory; level `solid` controls collision) |
| `surface` | string | optional | none | materials/textures; `wall`/`floor`/`ceiling` documentation/validation |
| `texture` | string | **required for `material`** | — | materials. Logical id of a `texture` asset. Not allowed on other types. |
| `tile_metres` | number | optional | `2.0` (`0.05`–`64`) | materials only |
| `tint` | `[r,g,b]` | optional | `[1,1,1]` (channels `0`–`1`) | materials only |
| `entity_type` | string | optional | none | entities |
| `description` | string | optional | none | all |
| `tags` | array of strings | optional | `[]` | currently unused |

The loader also accepts the legacy `props` array (old flat registry) and merges it
with `assets`; new content should use `assets` only.

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

* Rust runtime (permissive): unknown classes/types parse; malformed `size`/`color`
  silently drop to fallbacks; duplicate ids are **rejected**.
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
| `material` | Surface appearance definition | `source: "definition"`, no file; names a `texture` | No | `defaults`, rooms, walls/`faces`, patches, regions |
| `texture` | A surface PNG | `model` = `.png` | No | a `material`'s `texture` field |
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
* Attributes: `POSITION` (required, vec3), `TEXCOORD_0` (required, vec2), `COLOR_0`
  (optional, vec4; absent = white). Indices 8/16/32-bit, but every index must fit
  16-bit addressing and the mesh is capped at 65 535 vertices.
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
| Triangles — needs justification above | 800 (only `spooner-man` is allowlisted) |
| Triangles — shipped art budget (tooling hard max) | **1500** (`tools/props` refuses to build above it) |
| Triangles — still loads, with an art-budget warning | above 1500, up to 6000 |
| Triangles — engine hard ceiling | 6000 (`src/level.rs::MAX_PROP_TRIANGLES`; above it the model falls back to a box) |
| Vertices per model | 65 535 |
| Primitives / materials / images per model | 32 / 16 / 16 |
| Prop texture | 64×64 or 128×128 preferred; 256×256 shipped art max; engine ceiling 1024 (`MAX_PROP_TEXTURE_SIZE`), downscaled to the runtime budget at upload |
| Materials per prop | one per primitive; a multi-material model costs one draw range per material per batch |
| Distinct models per level | 256 |
| Summed baked prop vertices per level | 1 500 000 |

The GLB tools in `tools/props/` emit and enforce the art budget; follow it. A model
above the art budget may still load if the engine ceiling allows it, but it does not
match the project's visual language and the shipped-asset tests will flag it.

### Fallback behavior

Unknown catalog id → placeholder box (neutral size fallback
`[0.6, 0.9, 0.6]` m). Missing/malformed GLB or over-budget mesh → placeholder box plus
a one-time `[props]` warning. More than 256 distinct models, or exhausting the level
prop-vertex budget → later placements fall back to boxes.

### Adding a New Prop

1. Build the model with the Python toolkit (the intended route):
   * add `def build_<name>(p)` to `tools/props/parts/<module>.py` following the
     commented exemplar `tools/props/parts/utility.py`, and register it in that
     module's `PROPS` dict;
   * a new module must be listed in `tools/props/parts/__init__.py`;
   * use `p.set_texture(64|128)` and paint with `p.tex` (or `tex.auto`); build with
     `p.box`/`p.cylinder`/`p.tube`/`p.plane`.
2. Add the catalog entry under `assets[]`:
   `id`, `display_name`, `asset_class`, optional `theme`, `asset_type: "prop"` (or
   `"entity"`), `source: "file"`, `model` relative `.glb` path, `size` `[w,h,d]`,
   `color` `#rrggbb`, `category`, `solid`.
3. Build it: `python3 tools/props/build.py --only core:your_prop`.
4. Preview it: `python3 tools/props/preview.py --only core:your_prop` and inspect
   `target/prop-previews/your_prop.png`.
5. Validate:
   ```sh
   python3 tools/props/build.py --check
   python3 tools/assets/validate.py
   cargo test --workspace
   ```
6. Refresh editor thumbnails if you want them: `python3 tools/props/build.py --thumbs`.

A hand-authored GLB is accepted by the runtime if it satisfies the profile above, but
the default `tools/props/build.py` run requires a registered Python builder for every
catalogued placeable, and `cargo test` enforces the origin/scale/budget conventions.
Do not modify `spooner-man` while authoring a map; it is a shipped entity.

---

## 17. Decals

A decal is a small decorative surface marking (a sign, a floor arrow, hazard stripes).
It is **separate geometry** laid on an existing surface, not a material edit. Its
depth handling is automatic: every decal is displaced 0.2 mm along its surface normal
and drawn with a polygon offset, so it never fights its parent surface. Do not author
epsilon offsets or per-decal depth tricks.

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `x`, `z` | number | **yes** | — | World centre. |
| `y` | number | no | `0.0` | Vertical centre. For `floor`/`ceiling` it is replaced by the real surface height; for walls it is the height on the wall. |
| `width` | number | **yes** | — | In-plane horizontal size, `> 0`, `≤ 10` m. |
| `height` | number | **yes** | — | In-plane vertical size, `> 0`, `≤ 10` m. |
| `rotation_degrees` | number | no | `0.0` | In-plane rotation about the surface normal. |
| `material` | string | **yes** | — | A catalog `decal` id. |
| `surface` | enum | **yes** | — | `floor`, `ceiling`, `wall_north`, `wall_south`, `wall_west`, `wall_east`. |

Rules:

* One flat surface per decal. A horizontal (floor/ceiling) decal that resolves to
  surfaces differing by more than 0.05 m is **rejected**
  (`Decal {i} spans a floor or ceiling height change …`). It may not straddle a
  recess edge or two rooms at different elevations.
* **No decals on gable ceilings** (`Ceiling decal {i} targets a gable ceiling …`).
* Keep decals inside the room that should light them; they receive that room's baked
  lighting.
* Unknown decal ids emit nothing (no error geometry).
* Decal sheets are alpha cut-outs; background alpha 0. Artwork is visible where
  alpha ≥ 0.5.

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

Places has **no dynamic lights, no shadows, no lightmaps and no shaders beyond one
texture-times-vertex-colour pass**. Lighting is baked once per level load into vertex
colours. Authors control it with fixture placement, fixture type, colour and
brightness — there is no level- or room-wide lighting override.

### The implemented lighting model

Every light is a generic engine-level source: a **shape** (point, rectangle, line), a
world position, an RGB colour, an intensity, a **range**, a **falloff** curve and an
`enabled` flag. Visible fixtures and placed props are ordinary objects that *own* zero
or more of these sources; nothing about a fixture family, prop model or material
creates or implies a light. Adding a new glowing object never means adding a new light
family.

```text
sample = room/partition-area baseline
       + visibility-tested local fixture pools
       + doorway-blend deltas
       clamped to [AMBIENT_LEVEL = 0.10, 1.0] per channel
```

1. **Room baseline.** Each room sums `intensity × height factor × colour` over the
   fixtures it owns, spreads it over its floor area, log-compresses it and maps it
   onto `[0.10, 1.0]`. A room with no fixtures sits at exactly the ambient `0.10`:
   unlit rooms are dark by design.
2. **Partitions.** If opaque internal walls split a room's footprint into
   disconnected areas, each area gets **its own baseline** from the fixtures it can
   reach. A wall that stops short of the ceiling is not a partition; a door header
   separates while a doorway keeps a bounded blend; a window does not connect
   baselines at all.
3. **Local fixture pools.** Each fixture adds a bounded local pool (brightness ×
   height factor × falloff), reaching up to 6 m and capped per channel. The pool is
   computed from the fixture's luminous rectangle/disc, so being near a bright
   fixture matters.
4. **Wall-boundary occlusion.** A pool only reaches what its fixture can see: light
   is tested against the exact wall solids. Doors, windows, passages and vents all
   transmit through exactly the hole they cut; a solid header/sill still blocks.
5. **Doorway baseline transfer.** Only `door` and `passage` openings whose sill
   reaches the lower connected floor blend a bounded amount of the neighbour's
   baseline through the aperture (radius 6 m, strength 0.5, fading above the header).
   Windows and vents transmit pools only; they never blend baselines.
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

* Density follows the quality profile: **Full** bakes 12 texels per metre onto up to
  two 1024-texel pages, **Low** bakes 8 texels per metre onto two 512-texel pages.
  Both profiles bake the same set of surfaces; faces longer than one chart are split
  automatically.
* `settings.json` carries `"lightmaps": true|false` (default `true`). The environment
  override `LIMINAL_NO_LIGHTMAPS=1` forces the historical vertex-lit path for a
  benchmark or A/B capture run.
* If a bake cannot fit the page budget, or an atlas page cannot upload, the level
  rebuilds with `LightmapMode::Off` and draws exactly the old vertex-lit colours — a
  level never renders black because of a lightmap failure.
* Fixtures, prop placeholder boxes and decals are always vertex-lit: their colour
  keeps the baked light folded in, exactly as before.
* Set `LIMINAL_DUMP_LIGHTMAPS=1` to write the baked atlas pages as PNGs under
  `target/agent-work/atlases/` for inspection.

#### Static props occlude the bake

A placed prop is not air: every prop's own triangles become a small set of
occlusion boxes for the bake, automatically and per distinct model. Nothing is
authored and there is no per-prop occlusion flag. The visible consequences:

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

#### Dynamic objects (demonstration only in this batch)

The engine has a separate render path for objects whose transform changes every
frame — moving components that must not be re-baked, re-batched or written into
the static lightmap. In this batch it is proven by one object in Places Demo: a
`core:washer_drum` turning in front of a placed `core:washing_machine` (the
machine itself is an ordinary static prop and participates in the bake).

* Dynamic objects are engine-created, not authored in level JSON. A level cannot
  place or drive one yet.
* They are lit by a single probe of the static bake at their current position
  (no shadows, no realtime lights), which is the documented temporary behaviour
  for Batch 3 to evolve.
* Moving one never rebuilds geometry, batches or lightmaps.

### Current Light Fixture Types

Generated from `src/lighting/tuning.rs::fixture_profile` and `assets/catalog.json` at
the documented commit. The fixture's **face PNG is data**; its **mesh family and
luminous footprint are code**.

| Fixture ID | Mount type | Visible artwork (PNG) | Shape / footprint | Important authoring notes |
| --- | --- | --- | --- | --- |
| `core:fluorescent_panel_01` | ceiling (default) | `environment/office/textures/lights/fluorescent_panel_01.png` (256×128) | Rectangle 1.2 × 0.6 m; rotation swaps axes | The default family **and the fallback for every unknown id**. Hangs 0.01 m below the local ceiling; under a gable it follows the eave/ceiling above its footprint. Also used as `pool:` restyle base in packs. |
| `core:pool_light_round` | ceiling | `environment/pool/textures/lights/pool_light_round_01.png` (128×128) | Disc, 0.44 m diameter; rotation-invariant | Round recessed downlight. Same ceiling-plane derivation as the panel. |
| `core:pool_light_wall` | **wall** — requires `"mount": "wall"` and a finite world `"y"` | `environment/pool/textures/lights/pool_light_wall_01.png` (128×64) | Rectangle 0.4 × 0.18 m centred on (x, y, z) | Faces `rotation_degrees`: 0 = +Z, 90 = +X, 180 = −Z, 270 = −X. Place the point on the wall plane; the body extends ~0.11 m forward. Light is emitted from the rectangle's front. |

Ceiling fixture rotation is quantised to a 0°/90° axis swap; only wall sconces rotate
continuously. Unknown fixture ids load as the office panel with the untextured white
sheet (no error); a named-but-broken sheet logs
`[fixtures] fixture {id} sheet {path}: {error}; drawing the untextured sheet instead`.

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
2. `src/render/fixtures.rs` — implement the family's emitter(s): lit faces into the
   `lit` batch, untextured housing into `housing`.
3. `src/render/geometry.rs` (`emit_fixtures`) — add the `match` arm that calls the new
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

No catalog/renderer change is needed for a **pack** to restyle the panel family: a
`.zip` level pack may reference `"fixture": "pack:<id>"` and ship its own PNG; `pack:`
ids always resolve to the office-panel geometry.

---

## 19. Prop Placement

Exact level syntax (all fields verified against `src/level.rs::PropDef`):

```json
{
  "model": "core:desk",          // REQUIRED. Logical catalog id. Unknown ids render a placeholder box.
  "x": 4.6, "y": 0.0, "z": 5.8,  // optional, default 0. y is an offset ABOVE the local walkable floor.
  "rotation_degrees": 180.0,     // optional Y rotation; model +Z faces this way at 0.
  "scale": 1.0,                  // optional, > 0. Scales model and explicit size.
  "size": [1.6, 0.75, 0.7],      // optional [w,h,d] metres for collision/placeholder.
  "solid": true                  // optional, default false. Only this flag creates collision.
}
```

Semantics:

* `y` is **not absolute world Y**: `base_y = walkable floor at (x,z) + y`. A negative
  `y` deliberately sinks a prop into the floor and is never corrected. On a room with
  `floor_y: -1.5`, a prop at `y: 0` stands on that room's floor.
* **Collision box = level `size` (or `PROP_FALLBACK_SIZE [0.6, 0.9, 0.6]`) × scale.**
  The catalog `size` is never used for collision. A solid prop that should block like
  its picture must author `size`.
* The collision box is **axis-aligned and does not rotate**. For a 90°/270° rotated
  solid prop, author the x/z-extents swapped.
* Rotation does rotate the rendered model around Y.
* `props` are never tested against their render mesh; intentional clipping and
  overlap are preserved.
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

A deliberate sunken prop (from the prop showcase fixture; the `id` key shown there is
an ignored extra — do not copy it):

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
  plane.
* Floor/ceiling decals snap to the real surface height under them and lift 0.2 mm;
  keep `y` consistent with the room for readability but it is replaced.
* Wall decals use the authored `y`.

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

All fixtures — ceiling and wall — live in the level's `ceiling_lights` array (the key
name is historical).

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `fixture` | string | **yes** | — | Fixture id from the catalog/registry. |
| `x`, `z` | number | **yes** | — | World position. |
| `rotation_degrees` | number | no | `0.0` | Y rotation. Ceiling families quantise to a 0°/90° axis swap; wall fixtures rotate continuously. |
| `brightness` | number | no | `1.0` | Alias `intensity`. Must be finite, `≥ 0`; baking clamps to 8. |
| `color` | `[r,g,b]` | no | `[1.0, 0.96, 0.88]` | Each channel `0`–`1`. Drives both the lamp face and the illumination. |
| `mount` | `"ceiling"` \| `"wall"` | no | `"ceiling"` | Wall fixtures require `y`. |
| `y` | number | no (required for wall) | derived for ceiling | Ceiling: optional mounting world Y (also selects a storey in stacked rooms). Wall: required world Y of the fixture centre. |
| `range` | number | no | `6.0` | Distance in metres at which this fixture's pool reaches zero. The falloff curve is evaluated over this range, so a shorter range is also a softer pool. |
| `falloff` | `"smooth"` \| `"linear"` \| `"constant"` | no | `"smooth"` | Pool decay curve. `constant` holds full strength to `range` then stops (a deliberately hard pool). |
| `enabled` | boolean | no | `true` | `false` keeps the fixture's visible glow but removes **all** of its environmental illumination. |
| `emission` | number | no | the fixture's `brightness` | Independent emissive strength of the visible face, `≥ 0`. Lets a face read brighter (or dimmer) than the light the fixture casts. |

Ceiling fixture, office default look:

```json
{ "fixture": "core:fluorescent_panel_01", "x": 21.5, "z": 2.4,
  "rotation_degrees": 0.0, "brightness": 0.34, "color": [1.0, 0.2, 0.15] }
```

Round pool downlight:

```json
{ "fixture": "core:pool_light_round", "x": 9.0, "z": 9.0,
  "brightness": 0.85, "color": [0.55, 0.78, 1.0] }
```

Wall fixture (must author `mount` and `y`; the point sits on the wall plane):

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
finite numbers between 0 and 1`). Unknown fixture ids are not rejected; they render and
bake as the office panel with the untextured sheet.

A fixture whose face should glow without lighting the room — a sign, a screen, a
decorative tube — authors its own emissive strength separately:

```json
{ "fixture": "core:fluorescent_panel_01", "x": 47.5, "z": 13.0,
  "brightness": 0.18, "emission": 1.0 }
```

That entry is real content in `places_demo.json`: the far east corridor's last tube
reads fully bright while still casting only its dim 0.18 pool.

### Lights owned by props

Any placed prop may own generic light sources. They are positioned in the prop's own
local frame (offset scaled with the prop, rotated by its yaw) and cast light through
exactly the same engine path as a fixture:

```json
{
  "model": "core:vending_machine", "x": 12.0, "z": 1.0, "rotation_degrees": 270.0,
  "lights": [
    { "shape": "rect", "half_width": 0.3, "half_depth": 0.05,
      "offset": [0.0, 0.9, 0.3], "intensity": 0.15, "range": 3.0,
      "color": [0.55, 0.78, 1.0], "falloff": "smooth", "enabled": true }
  ]
}
```

| Field | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `shape` | `"point"` \| `"rect"` \| `"line"` | no | inferred: `rect` when half extents are authored, `line` when `length` is, else `point` | Emitting shape. |
| `half_width`, `half_depth` | number | for `rect` | — | Half extents in the object's local X/Z, metres, `> 0`. |
| `length` | number | for `line` | — | Tube length along local X, metres, `> 0`. |
| `offset` | `[x, y, z]` | no | `[0, 0, 0]` | Centre of the emitter in the object's local frame; scaled with the object. |
| `rotation_degrees` | number | no | `0.0` | Yaw of the emitter relative to the object. |
| `color` | `[r, g, b]` | no | `[1.0, 0.96, 0.88]` | Each channel `0`–`1`. |
| `intensity` | number | no | `1.0` | Alias `brightness`; finite, `≥ 0`. |
| `range` | number | no | `6.0` | Pool radius in metres, `> 0`. |
| `falloff` | `"smooth"` \| `"linear"` \| `"constant"` | no | `"smooth"` | Pool decay curve. |
| `enabled` | boolean | no | `true` | `false` casts nothing. |

At most 8 lights per prop. A light with malformed shape dimensions, a negative
intensity, a non-positive range or a non-finite offset is a level error (the loader
reports the prop and light index). The prop's own **emission is a separate property of
its GLB material** — a light authored here is the only way a prop illuminates
anything, and a glowing material does not imply one.

---

## 22. Composition and Art Direction

Places is a slow first-person exploration game of quiet, over-lit institutional
interiors that stop being finished around you. Keep this practical:

* **Coherent low-poly environments.** Geometry, props and textures share one scale and
  one deliberate vocabulary. Do not mix photorealistic texture detail with crude
  boxes — the renderer has no PBR, no normal maps and no shadows to sell it.
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

There is no water, no glass, no transparency and no reflections. Implied water is
damp/damaged materials plus recessed geometry; implied glass is an empty window
aperture.

---

## 23. Building New Assets for a Map

Decision tree:

| Need | Use | Procedure |
| --- | --- | --- |
| A wall/floor/ceiling appearance | Material + texture | Add PNG → texture entry → material entry (recipes below). |
| A local sign or marking | Decal | Add POT RGBA cut-out PNG → `decal` entry → place in `decals`. |
| A three-dimensional object | GLB prop | Toolkit (`tools/props/parts/*.py`) → build → `prop` entry → place in `props`. |
| A light source | Existing fixture, or a new fixture family | Reuse a fixture id (`ceiling_lights`) or add a family (see [Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)). |
| Something mounted but not luminous | Prop | Model it as a GLB prop; there is no generic mounted-fixture system (no exit signs, fans or alarms as fixtures). |
| Arbitrary structural geometry | **Not supported.** | Only rectangular rooms, walls, openings, patches and regions exist. Shape the environment from these; use props for details. |

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

**Authoring rule:** one surface per plane. Do not author a second floor/wall to
overlap an existing one. Raised thresholds belong in `floor_regions`, not in a
duplicated cap. Adjacent rooms meeting at a doorway should have their floors meet at
the shared boundary; the renderer already prevents the wall cap from duplicating
them (commit `1783828`).

### Duplicate Wall Surfaces / Adjoining Walls

**Symptom:** z-fighting along a wall shared by two rooms; visible seam or flicker.

**Cause:** a continued wall emitted a second coplanar face over the shared span, or a
wall end cap/reveal was emitted under an abutting wall.

**Authoring rule:** author each physical wall once. The renderer coalesces walls that
share plane + thickness and overlap in length and height (last covering member's
material wins per cell) and clips hidden caps, but the correct authoring is a single
wall. Never place two walls with the same footprint.

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
touches no room; `tools/bench/check_holes.py` counts clear-colour pixels in captures.

### Invalid Wall or Opening Dimensions

**Symptom:** the level is rejected on boot (`Wall {i} opening {j} …`), or an opening
silently does nothing.

**Cause:** `offset + width > wall.length()`, negative `sill`, non-positive dimensions,
or an opening whose vertical span misses the wall.

**Authoring rule:** measure the wall first: length = larger of `width`/`depth`;
openings start at the min corner. Openings that miss the wall vertically are accepted
but produce no cut — check the numbers.

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

**Cause:** the wall authors a rigid `height` in a gable room, or its origin is not at
the room's floor level so it resolves the wrong ceiling.

**Authoring rule:** omit `height` for walls that should follow the local ceiling.
Author `height` only when you deliberately want a rigid wall (e.g. a half-height
partition), and remember it is measured from the wall's own `y`.

### Overlapping Rooms

**Symptom:** strange ownership behavior (geometry from room A, lighting from room B),
double floors, or odd fixture selection.

**Cause:** overlapping room footprints are legal but ownership differs between
geometry (first room in order) and lighting (smallest area).

**Authoring rule:** overlap only deliberately (stacked storeys, balconies) and keep
the overlap minimal. Give lights an explicit `y` when storeys share a footprint.

### Accidental Prop Intersections

**Symptom:** props intersect walls or each other, or a "solid" prop does not block.

**Cause:** props are placed by centre; collision boxes are axis-aligned and never
corrected.

**Authoring rule:** intentional clipping is allowed and preserved — check it is
intentional. For solid props, author `size` matching the rendered footprint, swapped
for 90°/270° rotations.

---

## 25. Common Lighting Mistakes

### Light Crossing an Opaque Wall

**Symptom:** a fixture appears to light the room behind a wall.

**Cause:** historically, local pools were distance-only. This is fixed: pools are
occlusion-tested against exact wall solids.

**Authoring rule:** trust the occlusion, but place fixtures inside the room they
should light. A fixture outside a room still lights the space it can see; fixtures
outside every room are defined but isolated.

### A Prop That Used To Be Lit Through Now Shadows

**Symptom:** after upgrading, a floor or wall behind a machine/cabinet is darker than
it used to be, and the object looks grounded instead of floating.

**Cause:** this is intended. Static props occlude baked light (Batch 2), derived from
the rendered model. A prop that stood in front of a fixture was previously lit as if
it were air.

**Authoring rule:** nothing to change — it is the desired result. If a space is now
too dark, add or brighten a fixture on the side that needs the light rather than
removing the prop. Note that a *non-solid* prop occludes too: occlusion follows the
drawn model, not the collision box.

### Lightmap Seam or Blotch

**Symptom:** a faint bright or dark line along where two walls meet, or a patch of
one surface's light bleeding into the next.

**Cause:** a lightmap chart boundary. Charts are padded and their gutters are filled
from the chart's own edge texels, so this should not happen; extra subdivision appears
where a merged quad was capped or a wall face was split.

**Authoring rule:** do not try to fix it from the level — there is no chart authoring
control. Report it as an engine bug with the level id and camera position. A
vertex-lit fallback always exists (`"lightmaps": false` in `settings.json` or
`LIMINAL_NO_LIGHTMAPS=1`), so a map is never blocked by a bake problem.

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
the ceiling under their footprint.

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

### Coloured Light Crossing an Opaque Wall

**Symptom:** a green room tints the room behind its wall.

**Cause:** historical distance-only pools. Fixed by exact occlusion applied to colour
as well as brightness.

**Authoring rule:** the rules are colour-symmetric now; still verify by viewing both
sides of a shared wall. A wall face is lit by the room it opens into.

### Vertical Light Leakage

**Symptom:** a lit lower storey brightens the sealed room above (or vice versa).

**Cause:** historically floors/ceilings were not blockers. Fixed by per-storey
interfaces and ceiling bodies.

**Authoring rule:** stack rooms freely; they are sealed. Raised platforms and lowered
basins inside one room stay connected — that is intended. If you want light to move
vertically, leave a genuine open side (an upper floor covering part of the footprint).

### Partitions

**Symptom:** a partitioned room's dark half still glows with the lit half's baseline.

**Cause:** historically one baseline per room footprint. Fixed: baselines flood-fill
around opaque internal walls.

**Authoring rule:** an opaque internal wall that reaches the ceiling partitions the
room; a wall that stops short of the ceiling does not, because the flood fill probes
just below the ceiling. (The bake also skips the flood fill entirely for rooms whose
walls only hug the boundary: a wall must cross more than 0.75 m into the room before it
is worth testing.) Doorways still blend a bounded amount by design.

### Doorway Transfer Assumptions

**Symptom:** light does not cross where an opening exists, or crosses a wall where a
header should block.

**Cause:** `window`/`vent` transmit pools but do not blend baselines; solid headers
block; only `door`/`passage` reaching the lower floor blend.

**Authoring rule:** use `door`/`passage` for real room connections where you want
baseline sharing; use `window`/`vent` for apertures that should only pass local
light. Coloured light is not a global wash: it comes from specific fixtures.

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

**Cause:** the `texture` id dangles, the `.png` is missing, truncated, or the wrong
dimensions; or a decal sheet is not a `.png`.

**Prevention:** run `python3 tools/textures/build.py --check` (existence, PNG
validity, 1024 hard limit) and keep the files in the repository.

### Incorrect Texture Dimensions

**Symptom:** tooling errors for >1024; warnings for >256 or non-POT fitted sheets;
stretching if a surface sheet is non-square.

**Prevention:** surfaces square, ≤1024 (256 preferred); decal sheets and fixture faces
POT; decals/fixtures are fitted, so author complete artwork with no bleed margin.

### Broken GLB

**Symptom:** `[props] prop model {path} is invalid: {reason}` and a placeholder box.

**Cause:** extensions, skins, animations, node transforms, >1 mesh/material, external
textures, non-triangle primitives, >65 535 vertices, over-budget triangles or texture.

**Prevention:** build props with `tools/props/build.py`, preview them, and run
`cargo test` so the shipped-asset checks enforce the conventions.

### Excessive Model Budget

**Symptom:** art-budget warnings; a model that does not match the low-poly visual
language; possible engine rejection.

**Prevention:** 500 triangles preferred, 800 justified, 1500 shipped art budget;
64–128 px prop texture preferred, 256 max. `spooner-man` is the only allowlisted
exception.

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
renders unexpectedly transparent (base pass has blending off, so surfaces should be
opaque).

**Prevention:** decal sheets are RGBA cut-outs with background alpha 0 and artwork
alpha 255; surface and fixture images are opaque.

---

## 27. Validation Workflow

Run from the repository root. Commands verified at the documented commit.

| Command | What it validates | Required for map authoring? |
| --- | --- | --- |
| `python3 tools/assets/validate.py` | Catalog parse; classes/types/sources; unique ids; every file-backed resource exists exactly once; shipped/drop-in/fixture levels reference declared ids; warns when a wall touches no room | **Yes** |
| `cargo test --workspace` | All 449 tests (448 pass, 1 ignored), including level/loader/render/collision/lighting suites and every audit | **Yes** |
| `cargo test surface_audit` | Coplanar architecture, doorway-threshold ownership, decal depth, wall junctions (Places Demo + fixed cases) | Strongly recommended |
| `cargo test lighting` | All lighting suites and audits: wall/colour occlusion, partition baselines, vertical isolation, the shipped demo's exact-visibility acceptance (matches `lighting::tests` and every `lighting_*` audit) | Strongly recommended |
| `LIMINAL_LEVEL=<id> cargo run` | Boots straight into the level and prints validation errors verbatim | **Yes, once per map** |
| `LIMINAL_CAPTURE=frame.png LIMINAL_LEVEL=<id> cargo run` | One-frame PNG capture for visual inspection | Useful |
| `python3 tools/textures/build.py --check` | Texture/decal/fixture PNGs exist, parse, ≤1024; warns >256 / non-POT | Yes for new art |
| `python3 tools/textures/seam_repair.py --check <png>` | Tiling seam metric per texture | Yes for new surface art |
| `python3 tools/props/build.py --check` | Every catalogued prop GLB exists and parses | Yes for new props |
| `python3 tools/props/preview.py --only <id>` | Renders a prop preview PNG | Recommended for new props |
| `python3 tests/test_package.py` | Package/repository gate (shipped-level checks, texture policy, catalog validation, README hygiene) | Recommended before shipping a map into `assets/levels/` |
| `python3 tools/bench/check_holes.py <capture.png>` | Counts clear-colour pixels (missing shells) in a capture | Useful |
| `python3 tools/bench/visual_check.py --baseline … --current …` | Visual regression between two builds (needs a GL window) | Optional |
| `cargo test --release -- --nocapture lighting_benchmark_report` | Prints the bake/build budget table for representative levels | Optional |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Strict lints | Required before committing code (see `AGENTS.md`), not for JSON-only maps |
| `cd level-editor && npm test` | Legacy editor tests | **Not part of map authoring.** The editor is out of scope and stale for vertical keys. |

At the documented commit these pass: `validate.py` → 67 assets / 0 warnings, exit 0;
`textures/build.py --check` → 21 textures / 12 soft warnings, exit 0;
`props/build.py --check` → 30 props, exit 0; `cargo test --workspace` → 448 passed,
0 failed, 1 ignored; `tests/test_package.py` → exit 0.

### What validation does NOT exist

* No coplanar/z-fighting detector for arbitrary levels; `surface_audit` checks Places
  Demo and fixed cases only. Inspect your level manually.
* No headless JSON validator: opening bounds, elevation rules, gable rules, decal
  rules, limits and budgets are enforced only by the Rust loader at load time.
* No duplicate-level-id detection; two files may share an `id`.
* No spawn-inside-room validation (only the shipped-level package test checks it).
* No unknown-key detection — typo'd JSON keys are silently ignored.
* No automated collision-vs-render-mesh check; collision is authored data.
* No automated tiling/orientation/opacity checks for user artwork beyond the shipped
  checks above.

A level that fails validation is skipped at discovery without a console line. Always
boot with `LIMINAL_LEVEL=<id>` to see the reason.

---

## 28. Final Map QA Checklist

### Structure

- [ ] Spawn is inside a real room, at a sensible position, facing the intended direction.
- [ ] Every space the player can enter is fully shelled by walls (no void gaps).
- [ ] Rooms connect intentionally; every connection has an opening in the correct wall.
- [ ] Room elevations match the intended route; no unwalkable surprise cliffs (or the
      cliffs are intended and have real rims).
- [ ] Openings fit their walls: `0 ≤ offset`, `offset + width ≤ length`, `sill ≥ 0`,
      positive dimensions.
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

### Materials

- [ ] Every material id exists in the catalog and resolves.
- [ ] The intended floor/wall/ceiling material is on the intended surface (check room
      overrides, wall `faces`, patches, regions).
- [ ] Recesses author an `edge_material`.
- [ ] Textures tile correctly at the intended real-world scale (`tile_metres`).
- [ ] No visible unintended seams; new surface art passed the seam check.

### Props

- [ ] Each prop is at the intended position and `y` (props stand on the local floor;
      sinking must be deliberate).
- [ ] Each prop faces the intended direction (`+Z` front at 0°).
- [ ] No accidental floating or sinking.
- [ ] `solid` matches intent; every solid prop authors a `size` that matches its
      rendered footprint (x/z swapped for 90°/270° rotations).
- [ ] No accidental prop-in-prop or prop-in-wall intersections.

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
- [ ] Emissive surfaces read bright in dark areas without brightening their
      neighbours.

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
- [ ] `cargo test --workspace` passes.
- [ ] The level boots with `LIMINAL_LEVEL=<id>` with no validation error.
- [ ] A capture (`LIMINAL_CAPTURE`) has been inspected if practical.

---

## Known Implementation Caveats

These are current, documented limitations or in-flight conditions that affect map
authoring. They are not invitations to change the engine as part of an authoring task.

1. **Quality profiles apply at level load.** `settings.json`'s `quality` value is read
   when the renderer is created; changing it mid-session does not re-scale textures
   already on the GPU. Set it, then load the level.
2. **Emission reaches surfaces and fixture faces, not decals.** A decal is drawn by
   its own pass, which has no emission term yet; an `emissive` material used as a
   *decal sheet* will not glow. Emission on wall/floor/ceiling materials and on GLB
   prop materials works.
3. **A light's range normalises its falloff.** `range` is the distance at which the
   pool reaches zero *and* the span the curve is evaluated over, so halving a range
   makes the pool both tighter and dimmer near the source. There is no separate
   "cutoff only" mode.
4. **Cone/spot lights are not implemented.** The generic model has point, rectangle
   and line shapes; a directional light needs a response model that is out of scope
   for this batch.
5. **Prop textures are downscaled, not re-authored.** A GLB may embed up to 1024 px
   per edge, but Full uploads a prop sheet at 256 and Low at 128; authoring big
   prop art gains nothing.
6. **`emission` on a fixture is emission only.** It never changes illumination; if a
   glowing face should also light the room, that is `brightness`/`enabled`.
7. **No `deny_unknown_fields`.** Misspelled or unsupported level keys are silently
   ignored: `"rotation": 90` does nothing, `"brightnesss": 0.5` does nothing. Diff
   against the schema skeleton in section 5.
8. **Invalid levels are silently dropped from the menu.** Boot with
   `LIMINAL_LEVEL=<id>` to see the validation error.
9. **No duplicate-level-id detection.** Two files may both declare `"id": "my_level"`;
   both appear, and `LIMINAL_LEVEL` picks the first discovered.
10. **Spawn outside every room is accepted** and falls back to floor `0.0`. Check it.
11. **`floor_patches` are unvalidated.** Malformed values are skipped at build time.
   Keep them well-formed and inside rooms.
12. **Documentation drift in shipped docs** (recorded here so agents trust the code):
   `assets/README.md` describes decal sheets as `CLAMP_TO_EDGE` (they are uploaded
   `REPEAT`); the prop exceedance allowlist lives in `src/props/tests.rs`, not
   `src/props.rs`; `README.md` counts "eight decal sheets" where the catalog has four
   decal ids; `src/level.rs`'s `y` comment says ceiling `y` is ignored, but the bake
   honours it.
13. **Legacy level editor is stale for vertical keys.** It does not round-trip
   `floor_y`, `floor_regions`, `ceiling`, light `mount`/`y`, and it rewrites a missing
   room `height` as 3.5 (the engine default is 4.0). Prefer editing JSON directly for
   those features.
14. **Tooling vs runtime strictness.** The Rust runtime is permissive (unknown
   class/type, missing `model`, invalid `size`, unknown fixture/prop ids degrade);
   `tools/assets/validate.py` is strict and fails. Pass the tool, not the runtime
   fallback.
15. **Generated fixture quirk:** `tests/fixtures/levels/prop_showcase.json` (generated)
    carries an `id` key on props that the level schema ignores. Do not copy it.
16. **`props/build.py --check` prints budget flags but does not fail on them**; budget
    enforcement lives in `cargo test`. Do not treat a clean `--check` as budget
    approval.
17. **No water, transparency or dynamic lighting.** "Flooded", "glass" and "mood
    lighting" must be expressed with existing materials, geometry and per-fixture
    colour/brightness.

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
  "size": [0.9, 2.0, 0.7], "solid": true,
  "lights": [
    { "shape": "rect", "half_width": 0.35, "half_depth": 0.05,
      "offset": [0.0, 1.2, 0.4], "intensity": 0.15, "range": 3.0,
      "color": [0.55, 0.78, 1.0] }
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
3. Reference it from a level (`defaults`, room, wall/`faces`, patch or region).
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

A lit sign face, using the same PNG as its own mask:

```json
{ "id": "hotel:sign_exit_01", "display_name": "Exit Sign Face",
  "asset_class": "environment", "theme": "hotel", "asset_type": "material",
  "source": "definition", "surface": "wall",
  "texture": "hotel:tex_sign_exit_01", "tile_metres": 1.0,
  "emissive": [0.35, 1.0, 0.45], "emissive_intensity": 2.0 }
```

## Add a new prop

1. Add a builder + registry entry in `tools/props/parts/<module>.py` (see
   `parts/utility.py`).
2. Add the catalog `prop` entry (`model` `.glb`, `size`, `color`, `category`,
   `solid`).
3. `python3 tools/props/build.py --only <id>`.
4. Preview with `python3 tools/props/preview.py --only <id>`.
5. Validate: `python3 tools/props/build.py --check`, `python3 tools/assets/validate.py`,
   `cargo test --workspace`.
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

## Add a new light fixture

A new *fixture id* always needs a code mesh family (section
[Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)):

1. Add the `FixtureKind` variant, index, profile and id mapping in
   `src/lighting/tuning.rs` (append; never reorder `index()`).
2. Implement the emitter in `src/render/fixtures.rs` and dispatch it in
   `src/render/geometry.rs::emit_fixtures`.
3. Add a POT opaque face PNG under
   `assets/environment/<theme>/textures/lights/`.
4. Add the `light` catalog entry with `model` = the PNG.
5. Update the family-pinning tests listed in the procedure.
6. If the family needs a pool shape that is not a rectangle, describe it in the
   family's `shape()` (the bake consumes it; no fixture-specific light code needed).
7. Validate: `python3 tools/textures/build.py --check`,
   `python3 tools/assets/validate.py`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace`.
7. Place it: `{ "fixture": "<id>", "x": …, "z": … }`
   (plus `mount: "wall"`, `y` for a wall family).

To reuse an existing family with new artwork, only steps 3–4 and 7 are needed, and
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
* validation commands or quality/budget rules.

The structure is deliberately table-based: a new capability should be inserted as a
row or subsection in the appropriate reference section — [Supported Asset Types](#15-supported-asset-types)
for a new asset type, [Current Light Fixture Types](#current-light-fixture-types) for a
new fixture family, [Adding a New Light Fixture Type](#adding-a-new-light-fixture-type)
for the procedure, and the recipe section for a new authoring workflow — rather than
rewriting the document.

When you update this guide:

1. Verify every changed claim against the runtime and tests, not against old prose.
2. Update the metadata block: the format versions and the last verified commit SHA
   (`git rev-parse HEAD`), and note which checks you ran.
3. Re-run the validation commands in [Validation Workflow](#27-validation-workflow) and
   fix any example that no longer matches.
4. Keep **Implemented Now** and **PLANNED** clearly separated. Never document a
   planned field, type or behavior as authorable, and never invent future JSON fields.
5. Keep normal game artwork as external image files in the asset tree; if internal
   generated diagnostics change, update the exceptions table in [Textures](#12-textures).

The metadata block is a verification marker, not a freshness guarantee: a
correct-looking hash does not make a stale claim true. Re-verify against the code.
