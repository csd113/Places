# Places

Places is the standalone home for Liminal, the game described below. Its Rust
crate, game assets, editor, tests and development tooling live at the repository
root so the project can evolve as a normal desktop game.

A slow first-person walking game: quiet residential interiors that keep going,
built from rectangular rooms, hallways and a baked static lighting system.

There is nothing to collect, fight or solve. You walk, and the building
changes around you.

## Levels

Three large, hand-authored residential levels ship with the app:

| Level | ID | Setting |
| --- | --- | --- |
| The Residence | `the_residence` | One very large house: entrance hall, living rooms, kitchen wing, bedroom corridors, service rooms and a back wing that has been leaking for years. |
| Quiet Apartments | `quiet_apartments` | An apartment building whose corridors and apartment interiors run into one another; the far apartments are only reachable through their neighbours. |
| After the Leak | `after_the_leak` | A house with a long-standing water problem spreading out of its service core; the last rooms are soaked and lit by two surviving fixtures. |

Each level is maintained near the spawn and decays as you walk: water staining
creeps along walls and ceilings, carpets turn damp, fixtures fail one by one and
furniture drifts out of place. The change is gradual, and the far end of each
level is dark but never unreadable. Older development levels (Level 1, the
asset demo, and the prop showcase/stress fixtures) are still installed and can
be chosen from the same menu.

## Assets

Levels reference assets by **logical id** (`core:desk`, `spooner-man`), never by
file path. The catalog at `assets/catalog.json` maps each id to its class
(`environment`, `entity`, `core`, `diagnostic`), its type (`prop`, `material`,
`texture`, `light`, `decal`, ...), an optional environment theme and the canonical resource
under `assets/`.

* The initial environment themes are **`office`** and **`pool`**. Themes group
  and document content; they never restrict placement, so Office and Pool assets
  (and entities) can be mixed freely in any level.
* **Environment surface artwork is external PNG.** A level names a *material*
  (`core:carpet_beige_01`); the catalog maps that material to a *texture* asset
  (`core:tex_carpet_beige_01`), which owns the PNG under
  `assets/environment/<theme>/textures/`. Editing the PNG and restarting shows
  the new pixels — no Rust change and no recompilation. See
  `assets/README.md` for the authoring workflow.
* **Spooner-Man** is an entity asset (`asset_class: entity`), not an Office or
  Pool prop. Its logical id is still exactly `spooner-man`, and its one
  canonical resource lives at `assets/entities/spooner-man/model/spooner-man.glb`.
* Generic props (couch, bed, appliances, ...) carry no theme and stay generic.
* The shipped surface PNGs are deliberately plain seed art that preserves the
  pre-4.5 look; the Goal 5 content pass replaces them with the real Office and
  Pool artwork.

`assets/README.md` documents the catalog format, the runtime resolution flow,
the material/texture split and the asset budgets. Validate everything with:

```sh
python3 tools/assets/validate.py        # catalog, resources, shipped levels
python3 tools/textures/build.py --check # surface PNGs and their budgets
python3 tools/props/build.py --check
cargo test
```

## Controls

Menus use `W`/`S` or `UP`/`DOWN` to move through items, `A`/`D` or
`LEFT`/`RIGHT` to adjust values, `ENTER` to activate and `ESC` to go back.

Gameplay uses these bindings (all of them can be changed in Settings):

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
| Pause menu | `ESC` |
| Performance overlay | `-` |

`Restore Default Bindings` in Settings puts this layout back after a rebind.
Custom bindings are saved to `settings.json` and are kept across launches.

The overlay prints frame timing, CPU/GPU load, submitted draw calls and the
baked-lighting summary; it is hidden by default.

## Running it

On macOS, install SDL2 and `pkg-config` if they are not already available:

```sh
brew install sdl2 pkg-config
```

Then launch the desktop development build from the repository root:

```sh
cargo run
```

The game resolves its levels, asset catalog, imported level packs and
`settings.json` relative to the repository root for development builds.

## Level packs

`levels/*.json` (and `levels/*.zip` level packs) are installed by copying them
into the `levels/` directory inside the package. Levels are validated on load;
a level that fails validation is skipped and reported on the console rather
than crashing the game.

## Desktop prerequisites

- macOS with SDL2 2.26.5 or newer and an OpenGL-capable driver.
- `pkg-config` so `sdl2-sys` can find the installed SDL2 library.

## Platform packaging

The game itself is platform-neutral. Historical PocketCHIP/Vitrallis packaging
artifacts are isolated in `platforms/pocketchip/vitrallis/`; they are not part
of the macOS development workflow. PocketCHIP packaging will be revisited as a
separate adaptation effort.

## Building

```sh
cargo build --release                  # development build for this machine
cargo test                             # level, lighting, renderer and format tests
python3 tools/assets/validate.py       # asset catalog and shipped-level validation
python3 tools/textures/build.py --check # surface texture PNGs and their budgets
python3 tools/props/build.py --check   # prop models exist and fit their budgets
```

Run the game and its tests from the repository root. Platform-specific build
and release steps belong under `platforms/` rather than in the game crate.

## Level format

The shipped JSON examples in `assets/levels/` demonstrate rooms, walls with door
and window openings, per-room and per-wall material overrides, floor patches,
ceiling lights and placed props. The bundled level editor writes the same format.

### Vertical geometry

A room is a rectangular volume with a floor plane, a ceiling profile and its own
base elevation. All three are optional in the JSON, and omitting them reproduces
the historical room exactly.

```json
{
  "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0,
  "height": 3.0,          // clear height from the floor to the eave
  "floor_y": 2.0,         // world Y of the floor plane (default 0.0)
  "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 }
}
```

* `floor_y` is world-space metres. A room at `2.0` sits two metres above a room
  at `0.0`; negative values are allowed. Floor, walls, ceiling, decals and props
  all resolve against it.
* `height` is the room-local clear height. Omitted, it is the standard default
  of **4.0 m**; legacy levels that write `3.5` keep 3.5.
* `ceiling.kind` is `flat` (default; one plane at `floor_y + height`) or
  `gable`. A gable needs `ridge` (`x` or `z`, the axis the ridge runs along) and
  `ridge_rise` (metres above the eave, strictly positive). The ceiling slopes
  linearly across the other axis, from the eave at both edges up to the ridge at
  the centre; ridge world Y is `floor_y + height + ridge_rise`. Gable-end walls
  follow the slope automatically when they do not author a height.

### Local floor regions

`floor_regions` add rectangular floor areas inside a room with their own
vertical offset. They are the general mechanism behind recessed pools, trenches,
sunken areas and raised platforms:

```json
"floor_regions": [
  { "x": 4.0, "z": 3.0, "width": 4.0, "depth": 3.0, "offset_y": -1.5,
    "material": "core:carpet_damp_01",
    "edge_material": "core:wallpaper_stained_01" }
]
```

* `offset_y` is relative to the containing room's `floor_y`; negative recesses,
  positive raises. A region's floor must stay below the room's eave.
* `material` overrides the region's floor (otherwise the room's floor material,
  or a floor patch covering the cell, applies); `edge_material` overrides the
  vertical transition faces (otherwise the room's wall material).
* Region edges are cut lines in the floor grid, so the boundary is exact and the
  region is never a second overlapping slab. Every height change gets real
  transition geometry, and a region touching a room boundary is closed against
  the room's floor plane.
* Collision agrees with the mesh: a height difference larger than a walkable
  step (0.4 m) is solid from the lower side and cannot be walked off from the
  upper side. Differing by less than a step, it is simply walked up or down.
* When regions overlap, the later entry wins, exactly like overlapping
  `floor_patches`. A region that lies outside every room, has a zero width or
  depth, or sits at or above the room's ceiling is rejected on load.

Unsupported today (documented, not silently accepted): non-rectangular regions,
ramps or slopes in a region, ceiling decals on a gable ceiling, decals whose
footprint straddles a floor or ceiling height change, and traversal between
stacked rooms. `assets/levels/vertical_diagnostic.json` demonstrates
every supported case and is the level to boot for visual checks.

The legacy level editor (still installed under `level-editor/`) does not author,
preview or preserve these keys: it opens such a level but its save drops them. A
replacement editor is planned; edit vertical-geometry levels as JSON until then.
