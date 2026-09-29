# Pumpkin-Head Skeleton (`pumpkin-skeleton`)

A walking human skeleton whose skull is replaced by a glowing pumpkin head,
modelled and rigged outside the repository's toolkit and registered as an
ordinary catalogued entity. The ordinary `skeleton` asset is unchanged and
separate: this variant is the walking Halloween creature.

## Geometry and rig

* 1 metre = 1 unit, **+Y up, +Z front** at `rotation_degrees = 0`; the origin
  is the floor-contact point between the feet.
* Bind-pose bounds `x[-0.227, 0.215] y[0, 1.808] z[-0.245, 0.158]`
  (catalog size `[0.442, 1.808, 0.403]`).
* 2278 triangles, one mesh, 95 nodes and one skin with 94 joints: the 24-bone
  human hierarchy (`root`, `pelvis`, spine/ribs, arms, legs) plus 70 `piece_*`
  rigid parts (the mesh node is not a joint).
* The pumpkin head is the vertices dominated by the `piece_pumpkin_head` joint
  (a child of `head`). `tools/entities/author_halloween_assets.py` partitions
  exactly those triangles into a second primitive with the `pumpkin_head`
  material; the body keeps the original `pumpkin-skeleton` material. The two
  primitives draw the original triangle set and share the same attributes and
  skin, so poses are unchanged.

## Clips

| clip | duration | loop | kind | reference speed |
| --- | --- | --- | --- | --- |
| `walk` | 1.4 s | yes | walk | 0.571 m/s |
| `collapse_reassemble` | 12.0 s | no | gesture | — |

`walk` is in place: a `move_to` route step or an AI walk state drives the
horizontal motion at 0.571 m/s and the clip plays at rate 1.0 with planted
feet. `collapse_reassemble` is a one-shot gesture and is deliberately excluded
from loop-seam checks.

## Material contract

`tools/entities/author_halloween_assets.py` gives the `pumpkin_head` primitive
a mild warm emissive factor `[0.6, 0.24, 0.07]`; the bones stay opaque and
unlit. The emissive head is a surface effect only — its illumination is the
runtime `glow` component.

## Runtime contract

* Place it with `"model": "pumpkin-skeleton"`, `"solid": false`, a `nav_agent`
  body (`radius` 0.25, `height` 1.81, `speed_mps` 0.571, `step_height` 0.4,
  `max_slope` 2.6667) and an `ai` component (`"behavior": "wanderer"` with
  `wander_radius` and `walk_speed` 0.571) for its bounded grass/tree patrol, or
  a `routes[]` entry; never both.
* Give it a `glow` component with `"socket": "piece_pumpkin_head"` (warm
  orange, small range) so the light follows the animated head, never a fixed
  world coordinate or the pelvis.
* Clip seams, skin weights and contact:
  `python3 tools/entities/validate_entities.py --glb assets/entities/pumpkin-skeleton/model/pumpkin-skeleton.glb`
  and `python3 tools/entities/check_clip_boundaries.py`.
* Material contract maintenance:
  `python3 tools/entities/author_halloween_assets.py --check`.
