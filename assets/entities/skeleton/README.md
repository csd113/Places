# Skeleton (entity)

A stylized articulated human skeleton prop: a readable skull, spine, rib cage,
pelvis, arms/hands and legs/feet on a real animation rig, with standing, floor
and chair sit poses.

| field | value |
| --- | --- |
| logical id | `skeleton` |
| class | `entity` |
| type | `entity` |
| entity type | `character` |
| theme | none (entities belong to no environment theme) |
| canonical resource | `model/skeleton.glb` |
| source texture | `textures/bone_01.png` (embedded into the GLB at build time) |
| catalog `size` | `[0.4266, 1.72, 0.315]` m (bind-pose box, arms at the sides) |

## Model

One mesh, one primitive, one material, one embedded 256×256 bone sheet, one
24-joint skin. 1492 triangles, 3204 vertices. The bind pose is a 1.72 m
standing figure, feet on `y = 0`, `+Z` front. The skull and jaw, a segmented
vertebral spine, five separated rib pairs, a pelvis, knobbed long bones,
fingered hands and feet are geometry. Eye sockets, nose and teeth are painted
onto the face surface. The shoulder span and height
follow the mannequin's proportions; the limbs retain narrow bone shafts and
wider joints. The segmented spine stays within the rib cage, and facial art
follows the curved skull surface. The tapered feet share the mannequin's
silhouette. The bone sheet's four cells
(`skull`/`face`/`bone`/`dark`) are painted with mottling and darker recesses.

Joints (all rest rotations identity; the shape is in the child translations):

```
root — pelvis — spine_01 — spine_02 — chest — neck — head — jaw
                                          ├ shoulder_l — upperarm_l — forearm_l — hand_l
                                          └ shoulder_r — upperarm_r — forearm_r — hand_r
       pelvis ├ thigh_l — shin_l — foot_l — toe_l
              └ thigh_r — shin_r — foot_r — toe_r
```

Weights are chain-restricted (a rib cannot follow an arm); the skull, jaw,
pelvis, sternum, vertebrae, clavicles, hands and feet are explicitly
pinned. The build aborts if any vertex reaches a joint outside its own chain.

## Clips

| clip | kind | loop | pose |
| --- | --- | --- | --- |
| `pose_stand` | `pose` | yes | the bind pose; pelvis at 0.920 m, soles at 0.000 |
| `pose_sit_floor` | `pose` | yes | sitting on the floor; pelvis 0.060 m, hips 135°, knees 93.5°, soles flat |
| `pose_sit_chair` | `pose` | yes | sitting on a 0.45 m seat; pelvis 0.450 m, hips 96.1°, knees 83.9°, shins vertical, soles flat, skull top 1.250 m |

Each clip is a single-key hold (with a root translation for the two sits) and
`kind = "pose"`; a `play_animation` action switches poses and the character
path crossfades between them.

## Chair fit

The chair pose is built against the shipped `core:chair` (seat pad top
0.465 m, backrest front at local `z = -0.1449`). Place the chair at
`(cx, y=0, cz)` with yaw `θ` and the skeleton at

```
x = cx + dz · sin(θ),  z = cz + dz · cos(θ),  yaw = θ,   dz = +0.0025 m
```

so the pelvis (lowest point 0.440 m) sits on the pad, the torso rear clears
the backrest by 3.1 cm, the knees pass 0.19 m beyond the seat edge and the
toes stay 0.20 m clear of the chair base. The pelvis/thigh volume sinks
0.025 m into the 3 cm seat cushion — inherent to a 0.45 m pelvis contract with
the feet on the floor; placing the chair at `y = -0.040` avoids it if a map
prefers no interpenetration.

`tests/fixtures/levels/entity_showcase.json` places a chair and the skeleton
with exactly this offset, and a second skeleton on the floor.

## Placement

Not solid; place through the ordinary prop system. Author the pose selection
as a `play_animation` interaction or a route step:

```json
{ "id": "skeleton_seated", "display_name": "Skeleton", "model": "skeleton",
  "x": 13.0, "z": 14.5025, "rotation_degrees": 180.0,
  "size": [0.43, 1.72, 0.32], "solid": false,
  "interaction": { "prompt": "Sit", "actions": [
    { "action": "play_animation", "clip": "pose_sit_chair" } ] } }
```

## Build

```sh
python3 tools/entities/build_skeleton.py --no-preview
python3 tools/entities/rig.py --check assets/entities/skeleton/model/skeleton.glb
```

Deterministic (two builds byte-identical), single process. The script
re-reads the written GLB and evaluates every pose offline (sole contact,
pelvis heights, hip/knee/ankle angles, weight sums, no degenerate triangles)
and prints the measured `core:chair` seat-fit offsets.

## Provenance

Generated in this repository by `tools/entities/build_skeleton.py` (pure
Python, stdlib only) with the shared `tools/entities/rig.py` writer and the
repository texture painter `tools/props/tex.py`. No third-party model,
texture or licence is involved; the source PNG and the build script are the
editable sources. The chair-fit measurements were taken from the shipped
`core:chair` GLB, which the pose does not modify.
