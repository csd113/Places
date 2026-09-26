# Rat (entity)

An animated low-poly rat: articulated legs, paws and a five-segment tail, with
looping idle, walk and run clips whose stride speeds are measured and
declared.

| field | value |
| --- | --- |
| logical id | `rat` |
| class | `entity` |
| type | `entity` |
| entity type | `character` |
| theme | none (entities belong to no environment theme) |
| canonical resource | `model/rat.glb` |
| source texture | `textures/rat_fur_01.png` (embedded into the GLB at build time) |
| catalog `size` | `[0.095, 0.139, 0.613]` m (bind-pose box, tail included) |

## Model

One mesh, one primitive, one material, one embedded 256×256 fur sheet, one
25-joint skin. 1482 triangles, 1925 vertices. The bind pose rests on `y = 0`,
is horizontally centred under its bounding box, and faces `+Z`.

The geometry is one connected, closed, consistently wound surface. Exact
booleans remove internal intersection faces and weld the ears, neck, muzzle,
limbs and tail into the body. UV splits retain identical skin weights, so
those boundaries cannot separate in animation. The original texture is preserved.
The checker audits exported topology, degenerate triangles and seam weights.

Joints (all rest rotations identity; the skeleton shape is in the child
translations):

```
root
└ pelvis — spine — chest — neck — head — ear_l / ear_r
           │        ├ leg_fl_upper — leg_fl_lower — leg_fl_paw
           │        └ leg_fr_upper — leg_fr_lower — leg_fr_paw
           └ leg_rl_upper — leg_rl_lower — leg_rl_paw
             leg_rr_upper — leg_rr_lower — leg_rr_paw
             tail_01 — tail_02 — tail_03 — tail_04 — tail_05
```

The tail runs far behind the body, so the bind-pose box centre sits about
0.12 m behind the torso centre: place the origin, not the nose. The clips can
swing the tail past the bind-pose box; the character path's culling expansion
already covers that.

## Clips

| clip | duration | kind | loop | reference speed | articulation |
| --- | --- | --- | --- | --- | --- |
| `idle` | 2.40 s | `idle` | yes | — | breathing body, head turn, tail sway; paws planted |
| `walk` | 0.40 s | `walk` | yes | **0.1985 m/s** | diagonal-pair gait (leg joints 43°/37°/41°), body bob, tail sway |
| `run` | 0.24 s | `run` | yes | **0.5735 m/s** | longer stride (62°/80°/58°), streamed tail, brief suspension with up to 8.9 mm floor clearance |

The reference speed is the ground speed the clip's stance sweep is authored
for, measured offline from the written GLB and declared in
`asset.extras.places_entity_clips` (the build re-measures it and asserts the
declaration). The runtime plays a `move_to` route at
`speed / reference_speed`, picks `run` once the route speed reaches 1.5× the
walk reference (0.298 m/s), and a planted paw drifts at most 0.1 mm in both gaits through mid-stance at the declared speed.

Every clip is loop-closed (each channel's first key equals its last) and uses
LINEAR sampling only, with 48 intervals per cycle. Idle ear motion is
periodic, and the swing starts at the same lift height as toe-off.

## Placement and routes

Not solid; place through the ordinary prop system with `size` matching the
catalog (or a slightly padded box for a friendlier `E` aim). A route drives
walking and running:

```json
{ "id": "rat_1", "loop": true, "steps": [
    { "step": "move_to", "x": 9.0, "z": 6.0, "speed": 0.199 },
    { "step": "move_to", "x": 9.0, "z": 8.0, "speed": 0.199 },
    { "step": "move_to", "x": 5.0, "z": 8.0, "speed": 0.573 },
    { "step": "move_to", "x": 5.0, "z": 6.0, "speed": 0.573 },
    { "step": "wait", "seconds": 0.5 }
  ] }
```

`tests/fixtures/levels/entity_showcase.json` places two independent rats with
routed walks and runs plus label interactions.

## Build

```sh
python3 tools/entities/build_rat.py          # write the shipped GLB + source PNG
python3 tools/entities/build_rat.py --check  # re-verify the written asset
python3 tools/entities/rig.py --check assets/entities/rat/model/rat.glb
```

Rebuilding requires Blender on `PATH` for the exact surface union. The
script is deterministic for the same Blender version (verified with 5.2.2), and prints counts plus its own offline skinning/contact/speed
verification. The development report and optional `--preview` PNG are written
under `target/entity-specialists/rat/`, not into the asset tree.

## Provenance

Generated in this repository by `tools/entities/build_rat.py` and the offline
Blender helper `tools/entities/rat_surface.py`, with the shared `tools/entities/rig.py` writer and the
repository texture painter `tools/props/tex.py`. No third-party model,
texture or licence is involved; the source PNG and the build script are the
editable sources.
