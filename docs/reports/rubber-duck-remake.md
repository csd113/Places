# Rubber duck remake

Replaced the duck with a continuous low-poly body/neck/head shell, broader
orange bill, raised tapered tail, shallow moulded wings and small dark eyes.
The flat base, +Z facing direction, catalog ID and floating placement behavior
remain unchanged. Actual bounds are 0.10124 × 0.12 × 0.145 m, within the
0.10 × 0.12 × 0.14 m catalog tolerance.

Created a new opaque 256×256 PNG atlas using the built-in image generation tool
(imagegen skill), then downsampled the generated raster with `sips`. The existing
2×2 body/head/beak/eye region layout is preserved: clean golden yellow,
lighter yellow, orange and charcoal. No runtime texture generation. No new
rig or animations; floating motion remains controlled by the existing level setup.

Files changed or produced:

- `assets/environment/pool/props/models/rubber_duck.glb` — final 560-triangle model.
- `assets/environment/pool/props/models/rubber_duck.png` — new texture, embedded in GLB.
- `tools/props/parts/duck_remade.py` — reproducible model builder.
- `tools/props/parts/pool.py` — routes the duck entry to the new builder.
- `docs/ASSET_SPECIFICATION.md` — records the new duck texture contract.
- This report and `docs/reports/rubber-duck-remake/{sheet_1.png,validation.json,build-validation.txt}`.

Validation passed:

```sh
python3 tools/props/build.py --only core:rubber_duck
python3 -m py_compile tools/props/parts/duck_remade.py
```

Additional GLB audit: zero open/non-manifold edges, degenerate triangles,
reversed faces or degenerate UV triangles; finite UVs inside 0..1; embedded
texture pixels match the source PNG. The three-view preview was inspected.
560 triangles is slightly above the preferred 500, below the 800 review threshold.
Intersecting closed accessory shells are intentional. No Rust tests or in-engine
playtest were run. All requested work is complete.

## Texture generation prompt

Mode: built-in image generation, opaque new image. Final prompt:

Use case: stylized-concept. Create a flat square opaque game texture atlas, NOT a render of a duck. Four equal square quadrants in a precise 2 by 2 grid filling the entire image, no borders or gutters. Top left: uniform warm golden yellow rubber (#f4c52c), extremely subtle broad tonal variation only. Top right: matching slightly lighter warm yellow rubber (#f7cf39), nearly flat. Bottom left: warm orange rubber (#e98a22), nearly flat. Bottom right: solid near-black charcoal (#202323). These are plain material color swatches for a low-poly PS1/PS2 rubber duck model. Matte clean toy rubber, stylized, quiet and readable at 256x256 resolution. No objects, eyes, circles, outlines, text, labels, watermarks, scratches, grime, grain, speckles, shine, lighting gradients or cast shadows. Exact quadrant boundaries at half image width and height.
