# Domestic model remakes

Remade all ten requested models in the existing Places style and catalog paths.
No catalog IDs, placement contracts, gameplay code, or unrelated models changed.
All models remain triangulated GLBs with embedded PNGs and baked face shading.

| Asset | Geometry changes | Triangles |
|---|---|---:|
| Knife | Shaped octagonal handle, bolster, broad asymmetric blade with a rounded nose | 188 |
| Fork | Tapered handle, curved shoulder, four individually tapered rising tines | 320 |
| Spoon | Shaped handle/neck and an elongated concave bowl with thickness | 336 |
| Plate | Defined foot ring, broader flat rim, shallow well and closed underside | 400 |
| Bowl | Broader upper body, rolled lip, thinner inner wall and stable foot | 480 |
| Wall switch | Bevelled plate and raised housing, slotted screws, shaped moving rocker | 232 |
| CRT TV | Closed tapered rear cabinet, chamfered bezel, convex glass, controls and feet | 516 |
| Couch | Three separate seat/back cushions, sloped upholstered arms, tapered feet | 544 |
| Bed | Timber rails/headboard, bevelled mattress/pillows, folded blanket with side drops | 532 |
| Office chair | Five-star base, horizontal casters, contoured waterfall seat and padded back | 524 |

The four larger props slightly exceed the preferred 500-triangle target but
remain below the 800-triangle review threshold. The bed retains the catalog's
low 0.55 m height. Existing material colors and texture pixels are preserved;
this pass changes the models and UV mapping, not their painted artwork.

## Files produced or changed

GLBs replaced:

- `assets/environment/home/props/models/knife.glb`
- `assets/environment/home/props/models/fork.glb`
- `assets/environment/home/props/models/spoon.glb`
- `assets/environment/home/props/models/plate.glb`
- `assets/environment/home/props/models/bowl.glb`
- `assets/environment/home/props/models/wall_switch.glb`
- `assets/environment/home/props/models/crt_tv.glb`
- `assets/core/props/models/couch.glb`
- `assets/core/props/models/bed.glb`
- `assets/environment/office/props/models/chair.glb`

New file-backed sources extracted from the previous models without pixel changes:

- `assets/core/props/models/bed.png`
- `assets/environment/office/props/models/chair.png`

Authoring and validation:

- `tools/props/parts/domestic_remade.py`: new builders for the ten models.
- `tools/props/parts/{tableware,home,furniture}.py`: route only these catalog entries to the new builders.
- `tools/props/validate_domestic_remake.py`: repeatable geometry, texture and switch audit.
- `docs/ASSET_SPECIFICATION.md`: extracted texture sources and revised switch hinge.
- This report and `docs/reports/domestic-model-remake/`: three contact sheets, build log and geometry JSON.

## Animation

The wall switch retains the rigid `toggle` animation, `switch`, `lever_pivot`
and `lever` node names, 0.35-second duration and 30-degree travel. The hinge was
repositioned to fit the new housing. A 31-position sweep confirms the rocker
stays in front of the backing plate. No character rigs or clips changed.

## Validation

Passed:

```sh
python3 tools/props/build.py --only home:knife home:fork home:spoon home:plate home:bowl home:wall_switch home:crt_tv core:couch core:bed core:chair
python3 tools/props/validate_domestic_remake.py
python3 tools/props/preview.py --only home:knife home:fork home:spoon home:plate home:bowl home:wall_switch home:crt_tv core:couch core:bed core:chair --out docs/reports/domestic-model-remake --sheet --width 480 --height 480
```

All ten exports pass the authoring scale/origin, UV and budget checks. The
additional audit finds zero boundary edges, non-manifold edges, degenerate
triangles, contradictory components or reversed faces. Embedded texture pixels
match their source PNGs. Front, rear and overhead previews were inspected.
Meshes use intersecting closed component shells where parts join; they are
render assets, not single Boolean-unioned fabrication solids.

No Rust tests, builds or Clippy were run, as requested. No in-engine playtest
was performed. Nothing in this requested ten-model set remains unfinished.

Preview ordering, left to right: couch, office chair, bed, switch, CRT;
knife, fork, spoon, plate, bowl.
