# Carved Candle Pumpkin (`carved-pumpkin`)

A carved Halloween pumpkin with a candle inside, modelled and rigged outside the
repository's toolkit and registered as an ordinary catalogued entity.

## Geometry and rig

* 1 metre = 1 unit, **+Y up, +Z front** at `rotation_degrees = 0`; the origin
  is the floor-contact point under the shell.
* Bind-pose bounds `x[-0.355, 0.346] y[0, 0.654] z[-0.314, 0.321]`
  (catalog size `[0.701, 0.654, 0.635]`).
* 1498 triangles across two primitives: the shell (1456, opaque, shares the
  entity surface sheet) and the candle flame (42, `candle_flame`, emissive
  factor `[1.0, 0.4, 0.045]`).
* One skin, 4 joints: `root`, `body` (the shell/face), `stem`, `flame` (the
  candle flame and the natural light socket). Inverse bind matrices, UVs,
  vertex colours and clip channels are untouched by the repository's material
  tool.

## Clips

| clip | duration | loop | kind | reference speed |
| --- | --- | --- | --- | --- |
| `laugh` | 2.4 s | yes | idle | — |
| `hop_forward` | 1.2 s | yes | walk | 0.5 m/s |

`hop_forward` carries the jump arc in the `body` node's Y translation (the mesh
rises about 0.24 m; the floor-contact root stays put), so the runtime drives
horizontal movement with a route and the **clip** supplies the vertical arc —
never both. A `move_to` route step at 0.5 m/s plays it at rate 1.0; the
locomotion resolver selects a declared `kind: "walk"` clip when the model has
no clip literally named `walk` (see `docs/RENDERER.md` and the guide).

## Runtime contract

* Place it with `"model": "carved-pumpkin"`, `"solid": false` and a `routes[]`
  entry; give it a `glow` component with `"socket": "flame"` for the candle
  light. `laugh` is a sensible reversal/rest cue at the route endpoints.
* The emissive flame primitive is a surface effect only. Never author a static
  `props[].lights` entry at the pumpkin's spawn: a moving entity must not bake
  light into one place.
* Clip seams, skin weights and floor contact:
  `python3 tools/entities/validate_entities.py --glb assets/entities/carved-pumpkin/model/carved-pumpkin.glb`
  and `python3 tools/entities/check_clip_boundaries.py`.
* Material contract maintenance:
  `python3 tools/entities/author_halloween_assets.py --check`.
