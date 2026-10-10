# Beach fish

`beach:fish` implements concept inventory **B17**: a full faceted turquoise
body, yellow face and broad bands, black eyes, tall two-colour dorsal fin,
paired pectoral fins, lower fin and a visibly **forked turquoise tail**. It
faces +Z and uses metres/+Y up. The origin is horizontally centred at the
bind mesh's bottom; underwater placement supplies its desired water height.

| Property | Value |
| --- | --- |
| Model | `model/beach_fish.glb` |
| Bind size, metres | `0.294 × 0.377 × 0.638` |
| Geometry | 288 triangles, 864 vertices, one skin/material/embedded PNG |
| Rig | 6 joints: root/body/tail/dorsal and two pectorals |
| `swim` | 1.2 s loop; tail beat, pectoral motion, small body turn and 7 mm bob |
| Root motion | Local vertical bob only; no horizontal patrol |

The loop carries `places_entity_clips` v2 metadata, `kind: "swim"`,
`loop:true` and no ground reference speed. Every piece is rigidly weighted;
the flat geometric normals and closed fin shells preserve the low-poly style.
Ear-clipped caps keep the forked tail's centre notch open without bad winding.

A full 120 Hz sweep of the exported mesh checked loop closure, normalized
weights, fitted UVs, nondegenerate triangles and unchanged rigid edge lengths.
Its local bounds, rounded outward in metres, are:

| Pose | Minimum XYZ | Maximum XYZ |
| --- | --- | --- |
| `swim` | `[-0.15430, -0.00852, -0.31945]` | `[0.15250, 0.38430, 0.31901]` |
| Runtime envelope | `[-0.21906, -0.07828, -0.41686]` | `[0.21906, 0.45528, 0.41686]` |

These remain inside bind half extent × 1.15 + 0.05 m per axis, keeping the
existing bounds and lighting anchor valid. Provide at least 2 cm clearance
below the placement for the bob, and enough water above its 0.385 m posed
height. Use `solid:false`; ground routes would snap it to a floor. Native
water visibility, illumination and concept comparison are primary phase checks.

## Placement and initialization

This fragment assumes a water surface at world y=0 and a real submerged floor
at world y=-1.0. **Prop `y` is floor-relative**: `y:0.45` yields a world base
at -0.55 and a posed top below -0.165, clear of both floor and water surface.
Adjust these coordinates to the level's actual water/floor.
An explicit timer action starts the renderer cue; `animation.playing` alone
does not initialize named playback in the current engine.

```json
{
  "props": [
    {"id":"shallows_fish","model":"beach:fish","x":3,"y":0.45,"z":-6,
     "solid":false,"occludes":false,
     "components":[{"component":"animation","clip":"swim","looped":true}]}
  ],
  "timers": [
    {"id":"fish_init","seconds":0.01,"autostart":true,"repeat":false,
     "bindings":[{"on":"timer","actions":[
       {"action":"play_animation","target":"shallows_fish","clip":"swim","loop":true}
     ]}]}
  ]
}
```

## Artwork and deterministic export

`textures/surface_master.png` is the retained opaque 1024² source and
`textures/surface_01.png` its exact 4×4 box-average 256² native derivative.
The GLB embeds the native PNG bytes unchanged. The fitted 4-column × 2-row
layout is yellow/light-yellow/turquoise/blue, then fin/light-fin/eye/glint;
UVs retain four-native texel insets. Broad clean colour regions keep the
yellow/turquoise separation readable at Low texture quality.

```sh
python3 tools/entities/build_beach_fish.py --author-textures
python3 tools/entities/build_beach_fish.py
python3 tools/entities/build_beach_fish.py --check
python3 tools/entities/rig.py --check assets/entities/beach_fish/model/beach_fish.glb
```

The author flag intentionally repaints both PNGs offline. Normal export reads
committed artwork. `--check` builds in a private temporary directory, compares
bytes with the shipped GLB and repeats the complete geometry/clip/bounds sweep.
