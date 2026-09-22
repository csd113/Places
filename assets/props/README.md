# Core prop pack

`props.json` is the authoritative prop registry shared by the game
(`src/loader.rs` → `PropCatalog`) and the level editor
(`level-editor/js/props.js`). Everything in `models/` is generated from it by
`tools/props/build.py`, which refuses to ship a prop that breaks the rules
below.

## Registry format

```json
{
  "id": "core:chair",          // stable id; levels and editor data reference this
  "name": "Chair",             // human-readable name shown by the editor
  "category": "Furniture",     // Furniture | Appliances | Utility | Decorative | Other
  "size": [0.5, 0.9, 0.5],     // [width, height, depth] in metres
  "color": "#8a7a63",          // flat colour used for the editor's fallback box,
                               // the prop-browser swatch and the missing-asset placeholder
  "model": "models/chair.glb", // path relative to this directory; null = placeholder box
  "solid": true                // true: blocks the player with a catalogue-sized AABB
}
```

`size` is also the collision box: props are never collision-tested against their
render mesh. Intentional clipping (props sunk into floors, overlapping walls or
objects) is allowed and never corrected.

## Coordinate, scale and origin convention

* 1 model unit = 1 metre; the engine is right-handed, **Y up**, floors at `y = 0`.
* The origin sits on the floor-contact point, horizontally centred under the
  object's true bounding box.
* **+Z is the front**: fridge doors, the TV screen, the vending machine panel,
  the couch seat all face `+Z` at `rotation_degrees = 0`.
* A prop's bounding box must match the catalogue `size` within
  `max(2 cm, 6 % of the axis)`; `tools/props/build.py` fails otherwise.
* Levels place props with `x`, `y` (vertical offset, may be negative), `z`,
  `rotation_degrees` (Y), `scale` and an optional `size` override.

## Budgets (PocketCHIP / Mali-400, 480×272 display)

| budget            | value                                                      |
| ----------------- | ---------------------------------------------------------- |
| triangles         | 50–500 preferred, ≤800 acceptable, 1500 hard ceiling; props above 800 are allowlisted in `src/props.rs` with a written reason (`spooner-man`, a creature, needs 880) |
| texture           | 64×64 or 128×128 preferred, 256×256 hard ceiling           |
| materials         | exactly one diffuse texture per prop                       |
| draw calls        | one per distinct prop model per level (instances are baked) |

Baked vertex colours carry the per-face shading and contact darkening (the same
`PROP_FACE_SHADES` the old placeholder boxes used); the shader stays
`texture2D(u_texture, v_uv) * v_color`. No normal maps, no PBR extensions, no
alpha, no animation, no skinning, no morph targets.

## GLB profile

One scene, one node, one mesh, one primitive, one material, one embedded PNG
image. Attributes: `POSITION` (float32 vec3), `TEXCOORD_0` (float32 vec2),
`COLOR_0` (normalised uint8 vec4), 16-bit indices, `mode: 4` (triangles).
Self-contained: no external `.bin`, no external textures, no extensions.
Anything else is rejected by `src/props.rs` with an actionable message.

## Adding a future prop

1. Add the registry entry above (id, name, category, size, colour, model path).
2. Add a build function to `tools/props/parts/*.py` and register it in that
   module's `PROPS` dict (see `parts/utility.py` for the commented exemplar).
3. `python3 tools/props/build.py --only core:your_prop` — this enforces the
   scale/origin/UV/budget rules and writes the GLB.
4. `python3 tools/props/preview.py --only core:your_prop` and look at
   `target/prop-previews/your_prop.png` before trusting it.
5. `python3 tools/props/build.py` to refresh `prop_proxies.json` (editor
   preview geometry) and `cargo test` to run the asset validator.
6. Place it in the editor, save the level, and load it in the game.

Nothing here is required at runtime: the game loads ordinary packaged GLBs.

## Development fixtures and checks

Four demo levels exercise the pack (none of them touch `level1`):

* `levels/asset_demo.json` — **the walkable demo map**, discovered in the game's
  custom-level folder and shown in the level select menu. Four rooms around a
  corridor place every catalogue asset (all twenty core props plus
  `spooner-man`), and it exercises the whole level format: doorways, a wide
  passage, windows, a vent, twelve ceiling lights, several props standing on
  other props, and the worn material set (stained wallpaper, damp carpet,
  stained ceiling) that Level 1 does not use.
* `levels/asset_maintained.json` — the same building on the maintained material
  set (yellow wallpaper, beige carpet, panel ceiling). A level carries one wall,
  one floor and one ceiling material, so the two demos are how the maintained
  and water-damaged sets are compared in game.
  Regenerate both with `python3 tools/levels/build_demo_levels.py`.
* `assets/levels/prop_showcase.json` — every catalogue prop placed once
  (the twenty core props plus `spooner-man`), arranged as a domestic room plus a
  utility room, including one crate deliberately sunk into the floor and a box
  overlapping it.
* `assets/levels/prop_stress.json` — ~150 repeated placements across nine
  models, used to prove that instances share one decoded model, one texture and
  one draw call per model.

Regenerate both with `python3 tools/levels/build_demo_levels.py`.

Checks to run before shipping a prop change:

```sh
python3 tools/props/build.py --check          # files exist, parse and fit the budgets
python3 tools/props/build.py --thumbs         # refresh the editor's prop thumbnails
cargo test                                    # catalogue, scale, origin, UV and batching tests
cd level-editor && npm test                   # editor parses the proxies and draws real geometry
```

Useful developer-only run flags (they never affect normal play):

* `LIMINAL_LEVEL=prop_showcase` — boot straight into a level (handy on the
  PocketCHIP, where the menu is awkward over SSH).
* `LIMINAL_CAPTURE=frame.png` — render one frame and write it out, then exit;
  this is how prop rendering is inspected on hardware without a screenshot tool.
* `LIMINAL_SPAWN=x,z,yaw_degrees` (or `x,y,z,yaw`) — stand at a specific spot,
  e.g. in front of a prop that needs a close look.
