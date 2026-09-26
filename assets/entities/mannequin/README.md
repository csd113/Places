# Concrete Mannequin (entity)

A grey concrete artist's lay figure with three selectable poses on one rig.

| field | value |
| --- | --- |
| logical id | `mannequin` |
| class | `entity` |
| type | `entity` |
| entity type | `character` |
| theme | none (entities belong to no environment theme) |
| canonical resource | `model/mannequin.glb` |
| source texture | `textures/concrete_grey_01.png` (embedded into the GLB at build time) |
| catalog `size` | `[0.42, 1.72, 0.305]` m (bind-pose bounding box) |

## Model

One mesh, one primitive, one material, one embedded 256×256 concrete sheet,
one 23-joint skin. 1370 triangles, 2962 vertices. The rest pose is a
standing figure: 1.72 m tall, ~1:7 head-to-body, 0.42 m shoulder span, feet
flat on `y = 0`, `+Z` front. Each foot is one tapered mesh from heel to toe,
with a raised instep that meets the shin.

Joints (all rest rotations identity; the shape is in the child translations):

```
root
└ pelvis — spine_01 — spine_02 — chest — neck — head
                                        ├ shoulder_l — upperarm_l — forearm_l — hand_l
                                        └ shoulder_r — upperarm_r — forearm_r — hand_r
  pelvis ├ thigh_l — shin_l — foot_l — toe_l
         └ thigh_r — shin_r — foot_r — toe_r
```

The concrete sheet is one region painted with `tools/props/tex.py` (grey base,
value-noise mottle and grain, dark aggregate spots, pale flecks, faint damp
streaks and form seams). Every body part samples a proportional sub-rectangle,
so the grain scale stays roughly constant over the figure. Vertex colours are
near-neutral `(232, 231, 226)` so the painted grey is the albedo and the mesh
keeps the pack's baked face shading.

## Clips

Each clip keys all 23 joints once at `t = 0` (a single-key hold; `duration` 0),
`kind = "pose"`, `loop = true`. A `play_animation` action switches between
them; the character path crossfades from the current pose, so the transition
reads smoothly without a separate transition clip.

| clip | pose |
| --- | --- |
| `pose_stand` | the bind pose (arms hanging, legs straight) |
| `pose_arms_up` | both arms raised overhead (155° from hanging, 15° elbow bend); hands reach 2.105 m |
| `pose_arms_forward` | both arms extended forward to horizontal; hands reach `z = +0.675 m` |

## Placement

Place it through the ordinary prop system. It is not solid; the catalog size is
used for the aim box. To demonstrate the poses, author a
`play_animation` action on a placed instance (a prop interaction run with `E`,
or an area trigger):

```json
{ "id": "pose_dummy", "model": "mannequin", "x": 4.0, "z": 3.0,
  "rotation_degrees": 180.0, "size": [0.42, 1.72, 0.305],
  "interaction": { "prompt": "Arms up",
                   "actions": [{ "action": "play_animation", "clip": "pose_arms_up" }] } }
```

A route can also cycle the poses (`{ "step": "play", "clip": "pose_arms_forward",
"seconds": 2.0 }`), which is what
`tests/fixtures/levels/entity_showcase.json` does.

## Build

```sh
python3 tools/entities/build_mannequin.py
python3 tools/entities/rig.py --check assets/entities/mannequin/model/mannequin.glb
```

The script is deterministic (fixed seeds; two runs byte-identical) and prints
triangle/vertex/joint/clip counts plus an offline geometric check of every
pose (feet grounded, arm reach, bounded edge stretch, normalised weights).

## Provenance

Generated in this repository by `tools/entities/build_mannequin.py` (pure
Python, stdlib only) with the shared `tools/entities/rig.py` writer and the
repository texture painter `tools/props/tex.py`. No third-party model,
texture or licence is involved; the source PNG and the build script are the
editable sources.
