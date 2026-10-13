# Cockroach

`entity:cockroach` is a tiny Office creature with six articulated legs, paired
wing covers, a pronotum shield, two articulated antennae and short rear cerci.
The flattened brown silhouette is built from 39 closed, outward-wound solids.

## Runtime contract

- Model: `model/cockroach.glb`, one skinned mesh, primitive, skin and material.
- Geometry: 480 triangles, 1,440 vertices, 25 joints; one rigid joint influence
  per vertex. No morphs, scaling channels or external resources.
- Coordinate frame: metres, +Y up, +Z front, horizontally centred bounding box,
  lowest bind vertex at `y = 0`.
- Bind size: `[0.0874143732, 0.0185625857, 0.1321252692]` metres including antennae,
  tarsi and rear cerci. The body itself is approximately 0.088 m long.
- Opaque nonmetal material with roughness `0.48` for a restrained cuticle sheen.
  Geometry and per-part vertex pigment carry the detail; environmental lighting
  supplies its light and shadow response, with no emission or painted lighting.
  glTF `COLOR_0` is linear. The material's linear `baseColorFactor`
  `[0.2231106261, 0.1249638443, 0.0856918012, 1]` anchors the existing palette to
  the intended dark-brown `#462412` shell under unit white light. The source
  sheet's nearly white centre introduces less than one display-byte difference.
  The single embedded PNG is exactly the committed `textures/white_01.png`.
- Sheet: 32×32, square, opaque, non-tiling, clamped; every UV samples its centre.
  It has no fitted atlas regions. Texture memory is 4,096 decoded RGBA bytes.

| Clip | Loop duration | Reference ground speed | Motion |
| --- | --- | --- | --- |
| `idle` | 1.80 s | none | subtle body settling and independent antenna sweeps |
| `walk` | 0.44 s | 0.0454545455 m/s | alternating tripods, 12 mm stride, 2 mm foot lift |
| `run` | 0.22 s | 0.1212121212 m/s | alternating tripods, 16 mm stride, 3 mm foot lift |

The planted phase lasts 60% of each cycle; the opposite tripod is half a cycle
behind. The two-link leg solver places each ankle, and a third joint holds each
tarsus level. Reference speed is stride divided by planted duration. Clips use
LINEAR keys and include identical first and final poses. The root never moves;
world navigation and gait-rate scaling belong to the ordinary entity runtime.

## Rebuild and inspect

```sh
python3 tools/props/build_cockroach.py
python3 tools/props/build_cockroach.py --check
```

The normal build reads the committed PNG without modifying it. The pure-Python
check reads the written GLB with both repository model inspectors, verifies
positions, skin indices and weights, per-part pigment, linear material factor,
nonmetal shell response, UVs, PNG identity, opaque white pixels,
closed topology, floor alignment, length and triangle budget. It samples 33
poses per clip to detect floor penetration, stretching and endpoint mismatch.
It takes approximately 0.2 seconds and does not bake a map or run the player.
No Rust test or CI work depends on this authoring command.

At creation, the offline check reported maximum edge stretch 1.000000005,
worst floor error −0.0000069 m and endpoint mismatch below 0.000000001 m.
A repeat export produced identical bytes. Native appearance and navigation
remain part of the integrating level's explicit visual validation.

## Sheet provenance

The new native sheet is a uniform resize of the existing committed shared white
sheet. It adds no texture artwork and retains opaque 252–255 RGB pixels. This
keeps the entity within the model texture class in `docs/ASSET_SPECIFICATION.md`
§8 and the shared white contract in §12.2, without embedding the 4 MiB full-size
surface sheet. The original file was left unchanged.

```sh
sips -z 32 32 assets/shared/textures/white_01.png \
  --out assets/entities/cockroach/textures/white_01.png
```

Source PNG SHA-256:
`b8ece440e5c8cb37b58d8246bf4eb8e3bd6747d2103193cc96f208634bd1222f`

Native PNG SHA-256:
`68cecb89a6a560f93cdfe880754e98ad3a68feaeddfec44656c6e70f0fa4ec6f`

GLB SHA-256:
`2844f044a131e1639096613460cf8d303a196a96c2dcd6a87d890d5a8fff3940`
