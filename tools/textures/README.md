# Texture toolkit

Catalog sheet validation and explicit offline exports for surfaces, fitted
fixture faces, decals, skies and diagnostic PNGs. Read the
[asset specification](../../docs/ASSET_SPECIFICATION.md) for shape, alpha,
orientation and seam contracts; use
[Offline Tooling](../../docs/OFFLINE_TOOLING.md#asset-authoring) for module
registration and source ownership.

```sh
python3 tools/textures/build.py --check
python3 tools/textures/seam_repair.py --check assets/environments/office/textures/walls/*.png
python3 tools/textures/build.py --only core:decal_ceiling_vent_01
```

The first command validates headers/dimensions and catalog/manifest drift;
the seam command measures wrapped pixels. The final command exports one sheet.
Normal builders load committed artwork where defined; deliberate authoring
commands and legacy painters are listed in Offline Tooling. The dimension
mismatch guard requires `--force` before replacing a differently sized sheet;
it does not guarantee an unchanged export at the same size. Runtime always
loads repository PNGs.
