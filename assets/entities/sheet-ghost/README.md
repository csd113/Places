# Sheet Ghost (`sheet-ghost`)

A draped sheet ghost with a painted face, modelled and rigged outside the
repository's toolkit. It is registered as an ordinary catalogued entity and is
the translucent, fading Halloween entity.

## Geometry and rig

* 1 metre = 1 unit, **+Y up, +Z front** at `rotation_degrees = 0`; the origin
  is at the ground below the sheet, and the bind pose already floats (the sheet
  starts at ~0.17 m and reaches ~1.79 m).
* Bind-pose bounds `x[-0.517, 0.476] y[0.171, 1.790] z[-0.413, 0.303]`
  (catalog size `[0.994, 1.619, 0.716]`).
* 1270 triangles across two primitives; 3 joints: `root`, `body` (the sheet,
  ~1.3 m up) and `hem` (the swaying lower hem). The face is painted into the
  vertex colours (not the texture) on a raised patch at the front (+Z). The
  toolkit partitions the 216 dark face triangles into a second primitive with a
  much dimmer emissive factor, so the cloth glows while the eyes, nose and
  mouth read as darker holes.

## Clips

| clip | duration | loop | kind | reference speed |
| --- | --- | --- | --- | --- |
| `float_forward` | 4.0 s | yes | float | 0.3 m/s |
| `idle` | 4.8 s | yes | idle | — |

`float_forward` carries the vertical bob in the `body` node (about ±0.035 m)
and no ground travel, so a `move_to` route step drives the forward motion at
0.3 m/s and the clip supplies the bob. The locomotion resolver selects a
declared `kind: "float"` clip when the model has no clip literally named
`walk`.

## Material contract

`tools/entities/author_halloween_assets.py` installs the runtime contract on
the shipped GLB (geometry, UVs, vertex colours, skin and clips untouched):

* `alphaMode: "BLEND"` on both primitives — the importer classifies the
  materials as translucent and the character path draws them in the sorted,
  depth-write-disabled translucent pass. Per-instance opacity comes from the
  runtime `fade` component; the asset itself stays at opacity 1.
* `emissiveFactor: [0.14, 0.5, 0.58]` on the cloth and a much dimmer
  `[0.03, 0.1, 0.12]` on the face-feature primitive — a mild cyan glow over
  the sheet while the painted features stay readable. Neither lights anything
  by itself.

## Runtime contract

* Place it with `"model": "sheet-ghost"`, `"solid": false`, an optional
  `routes[]` entry through the grass, a `fade` component
  (`period_seconds`, optional `phase`, `min_opacity`, `max_opacity`) and a
  `glow` component (`"socket": "body"`, cyan, `"fade": true`) so the
  environmental pool fades with the sheet. Omit `phase` for a deterministic
  per-instance offset, or author distinct phases so several ghosts do not
  breathe in lockstep.
* Clip seams, skin weights and float clearance:
  `python3 tools/entities/validate_entities.py --glb assets/entities/sheet-ghost/model/sheet-ghost.glb`
  and `python3 tools/entities/check_clip_boundaries.py`.
* Material contract maintenance:
  `python3 tools/entities/author_halloween_assets.py --check`.
