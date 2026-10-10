# Beach seagull

`beach:seagull` implements concept inventory **B15**: a white faceted head and
body, grey folded wings, dark primary tips and tail, yellow tapered bill and
two yellow legs with separate toes. It faces **+Z**, uses metres and +Y up,
and has its origin at the horizontally centred foot-contact plane.

The spread bind pose measures the complete wing silhouette; initialize the
intended clip to show folded wings on a perched instance. The 13-joint rig has
root/body/neck/head/tail and mirrored wing/tip/leg/foot joints. Every closed
piece has rigid weight one, preserving clean faceting and construction depth.
The importer derives flat geometric normals from its triangle soup.

| Property | Value |
| --- | --- |
| Model | `model/beach_seagull.glb` |
| Bind size, metres | `1.096 × 0.747 × 0.886` |
| Geometry | 458 triangles, 1,374 vertices, one skin/material/embedded PNG |
| `idle` | 3.0 s loop; wings folded, subtle head/neck and tail motion |
| `fly` | 0.8 s loop; articulated wing beat, folded legs, restrained head motion |
| Root motion | None; flying placement stays at its authored world position |

Both clips carry `places_entity_clips` v2 metadata with `loop: true` and no
ground reference speed. A complete 120 Hz sweep of exported vertices checked
loop closure, normalized weights, finite fitted UVs, nondegenerate triangles
and rigid edge lengths. Bounds are local metres, rounded outward below:

| Pose | Minimum XYZ | Maximum XYZ |
| --- | --- | --- |
| `idle` | `[-0.22030, 0, -0.50178]` | `[0.22030, 0.74806, 0.44429]` |
| `fly` | `[-0.55736, 0.15477, -0.44320]` | `[0.55736, 0.74806, 0.44429]` |
| Runtime envelope | `[-0.68020, -0.10603, -0.55946]` | `[0.68020, 0.85303, 0.55946]` |

The envelope is measured bind half extent × 1.15 + 0.05 m per axis. Flying
does not move the bounds or lighting anchor beyond the placement. Use
`solid:false`, keep clear of roofs/poles and do not attach a ground route to
the flying instance. Full in-engine lighting and concept comparison belong
to the primary phase validation; these geometric checks do not replace them.

## Placement and initialization

This fragment shows a perched bird on an assumed walkable 1.1 m deck and an
airborne bird above a floor at world y=0. **Prop `y` is floor-relative**: the
perched bird uses `y:0`, resting on the deck's actual floor, and the airborne
bird uses `y:2.7`. Adjust the coordinates to real supported surfaces in Phase 3.
An explicit `play_animation` action initializes each instance; setting
`animation.playing` alone does not emit a renderer cue in the current engine.

```json
{
  "props": [
    {"id":"gull_perched","model":"beach:seagull","x":4,"y":0,"z":-4,
     "solid":false,"occludes":false,
     "components":[{"component":"animation","clip":"idle","looped":true}]},
    {"id":"gull_air","model":"beach:seagull","x":-3,"y":2.7,"z":-8,
     "solid":false,"occludes":false,
     "components":[{"component":"animation","clip":"fly","looped":true}]}
  ],
  "timers": [
    {"id":"gull_init","seconds":0.01,"autostart":true,"repeat":false,
     "bindings":[{"on":"timer","actions":[
       {"action":"play_animation","target":"gull_perched","clip":"idle","loop":true},
       {"action":"play_animation","target":"gull_air","clip":"fly","loop":true}
     ]}]}
  ]
}
```

## Artwork and deterministic export

`textures/surface_master.png` is a 1024² opaque authoring source;
`textures/surface_01.png` is its exact 4×4 box-average 256² native derivative,
embedded unchanged in the GLB. The fitted 4-column × 2-row atlas regions are
white/grey/light-grey/dark-tip, then bill/feet/eye/glint. UVs keep a four-native
texel inset. The gentle painted facets are albedo, with no baked lighting.

```sh
python3 tools/entities/build_beach_seagull.py --author-textures
python3 tools/entities/build_beach_seagull.py
python3 tools/entities/build_beach_seagull.py --check
python3 tools/entities/rig.py --check assets/entities/beach_seagull/model/beach_seagull.glb
```

The first command intentionally repaints both PNGs offline. Normal export
loads the committed native PNG and checks it against the master. `--check`
regenerates in a private temporary directory, compares the GLB bytes and
repeats the full geometry/clip envelope checks without changing shipped files.
