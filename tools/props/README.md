# Prop toolkit

Catalog-driven GLB export, geometry inspection and software previews. Read the
[asset specification](../../docs/ASSET_SPECIFICATION.md),
[environment profiles](../../docs/assets/ENVIRONMENTS.md), and the
[asset authoring workflow](../../docs/OFFLINE_TOOLING.md#asset-authoring)
before changing a model or its fitted PNG.

```sh
python3 tools/props/build.py --check
python3 tools/props/build.py --report
python3 tools/props/build.py --only core:table
python3 tools/props/preview.py --only core:table --out target/prop-previews
```

`--check` reads shipped containers and texture budgets; export and Rust checks
cover additional geometry/UV/origin contracts. `build.py` protects existing
hand-authored skins and clips unless explicitly forced. Inspect approved
hand-finished geometry in scratch before replacing it. Software previews show
bind-pose artwork and alpha; use native captures for lighting acceptance.
Writer APIs, source-loading helpers, repair scope and worker controls are
maintained in Offline Tooling rather than duplicated here.
