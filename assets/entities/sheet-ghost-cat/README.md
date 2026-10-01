# Sheet Ghost Cat (`sheet-ghost-cat`)

A small draped sheet cat with a painted face, modelled and rigged outside the
repository's toolkit. It is the sheet ghost's companion: the translucent,
proximity-fading Halloween entity.

## Geometry and rig

* 1 metre = 1 unit, **+Y up, +Z front** at `rotation_degrees = 0`; the origin
  is at the ground below the sheet, and the bind pose already floats (the hem
  starts at ~0.046 m and reaches ~0.369 m).
* Bind-pose bounds `x[-0.113, 0.113] y[0.046, 0.369] z[-0.288, 0.288]`
  (catalog size `[0.226, 0.323, 0.576]`).
* 5 joints (`root`, `body`, `hem`, `tail_base`, `tail_tip`) and one
  single-material primitive. The painted face is carried in the vertex
  colours.

## Clips

| clip | duration | loop | kind | reference speed |
| --- | --- | --- | --- | --- |
| `float_forward` | 3.0 s | yes | float | 0.2 m/s |
| `idle` | 3.6 s | yes | idle | — |

`float_forward` carries the vertical bob in the `body` node and no ground
travel, so a `move_to` route step drives the forward motion at 0.2 m/s and the
clip supplies the bob. The locomotion resolver selects a declared
`kind: "float"` clip when the model has no clip literally named `walk`.

## Material contract

`tools/entities/author_halloween_assets.py` installs the runtime contract on
the shipped GLB (geometry, UVs, vertex colours, skin and clips untouched):

* `alphaMode: "BLEND"` on the single material — the importer classifies it as
  translucent and the character path draws it in the sorted,
  depth-write-disabled translucent pass. Per-instance opacity comes from the
  runtime `fade` component; the asset itself stays at opacity 1.
* `emissiveFactor: [0.14, 0.5, 0.58]`, the same mild cyan family as the sheet
  ghost's cloth. It lights nothing by itself.

The cat has no face partition: the whole sheet shares one material, so the
painted features read through the same glow.

## Runtime contract

* Place it with `"model": "sheet-ghost-cat"`, `"solid": false`, an optional
  `routes[]` entry, a proximity `fade` component (`near_radius: 1.0`,
  `far_radius: 2.2`, `fade_out_seconds: 0.8`, `fade_in_seconds: 1.6`) and a
  `glow` component (`"socket": "body"`, cyan, `"fade": true`) so its light
  fades with the sheet.
* Clip seams, skin weights and float clearance:
  `python3 tools/entities/validate_entities.py --glb assets/entities/sheet-ghost-cat/model/sheet-ghost-cat.glb`
  and `python3 tools/entities/check_clip_boundaries.py`.
* Material contract maintenance:
  `python3 tools/entities/author_halloween_assets.py --check`.
