# Beach crab

`beach:crab` implements concept inventory **B16**: a broad coral-red faceted
carapace, two stalk eyes, eight angular walking legs and **two open pincers**.
It faces +Z, uses metres/+Y up and has its origin at the horizontally centred
toe-contact plane. Its 18-joint rig contains root/body, eight rigid leg chains,
two stalks and mirrored claw/palm/finger articulations. Closed pieces use
weight one; triangle soup supplies deliberate flat geometric normals.

| Property | Value |
| --- | --- |
| Model | `model/beach_crab.glb` |
| Bind size, metres | `0.832090 × 0.381 × 0.561201` |
| Geometry | 692 triangles, 2,076 vertices, one skin/material/embedded PNG |
| `idle` | 2.4 s loop; light carapace bob, eye sway, pincer motion |
| `walk` | 0.8 s loop; alternating planted/swing legs, restrained claw motion |
| Reference walk speed | `0.160624564 m/s`; exported stance sweep measures `0.160624557 m/s` |

Both clips carry `places_entity_clips` v2 metadata with `loop: true`; walk is
`kind: "walk"` and declares the measured stride reference. Every clip is in
place. During a planted stance, a lowest toe vertex travels backwards by
0.0770998 m over 60% of its 0.8 s cycle; the route advances the placement at
the matching mean speed. Swing arcs lift the unplanted legs. This is a compact
presentation gait, without an additional animal AI or patrol framework.

A full 120 Hz exported-vertex sweep checked loop closure, normalized weights,
fitted UVs, nondegenerate triangles, rigid edge lengths and no meaningful
floor penetration. Bounds below are local metres, rounded outward:

| Pose | Minimum XYZ | Maximum XYZ |
| --- | --- | --- |
| `idle` | `[-0.41605, 0, -0.28061]` | `[0.41605, 0.38414, 0.28828]` |
| `walk` | `[-0.42875, 0, -0.29840]` | `[0.42914, 0.38414, 0.28828]` |
| Runtime envelope | `[-0.52846, -0.07858, -0.37270]` | `[0.52846, 0.45958, 0.37270]` |

These fit bind half extent × 1.15 + 0.05 m per axis. Runtime/native lighting
and silhouette inspection are separate primary phase checks.

## Ground route recipe

The fragment assumes dry supported sand at y=0 throughout x=1..2/z=3. All
segments need the crab's resolved footprint clear of rocks, water and walls.
Use `solid:false`: a solid crab would block its own first route step. Leave
the route in control of walk/idle; a permanent `play_animation` override would
prevent those route cues from selecting its gait.

```json
{
  "props": [
    {"id":"sand_crab","model":"beach:crab","x":1,"y":0,"z":3,
     "solid":false,"occludes":false}
  ],
  "routes": [
    {"id":"sand_crab","loop":true,"steps":[
      {"step":"play","clip":"idle","seconds":2.4,"loop":true},
      {"step":"move_to","x":2,"z":3,"speed":0.160624564},
      {"step":"play","clip":"idle","seconds":2.4,"loop":true},
      {"step":"move_to","x":1,"z":3,"speed":0.160624564}
    ]}
  ]
}
```

For a stationary crab, use an `animation` component and explicit autostart
timer `play_animation` action for `idle`, following the seagull recipe.

## Artwork and deterministic export

The retained `textures/surface_master.png` is an opaque 1024² source;
`textures/surface_01.png` is its exact 4×4 box-average 256² native derivative,
embedded unchanged. Its fitted 4-column × 2-row atlas carries
coral/top/underside/leg, then claw/stalk/eye/glint, with four-native texel UV
insets. Broad clean colour facets follow the reference's red palette.

```sh
python3 tools/entities/build_beach_crab.py --author-textures
python3 tools/entities/build_beach_crab.py
python3 tools/entities/build_beach_crab.py --check
python3 tools/entities/rig.py --check assets/entities/beach_crab/model/beach_crab.glb
```

`--author-textures` deliberately paints the committed source/native artwork
offline. Normal export loads it. `--check` compares a fresh deterministic
export in a private temporary directory and repeats the full clip/geometry
sweep, including the exported foot-stride measurement.
