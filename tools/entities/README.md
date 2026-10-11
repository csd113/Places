# Entity toolkit

Rig/GLB writing, sampled skin/contact checks, clip-boundary validation and
Blender contact sheets. Current character and creature contracts live in
[entity profiles](../../docs/assets/ENTITIES.md); common image/UV contracts
live in the [asset specification](../../docs/ASSET_SPECIFICATION.md).

```sh
python3 tools/entities/rig.py --check assets/entities/rat/model/rat.glb
python3 tools/entities/validate_entities.py --workers 8
python3 tools/entities/check_clip_boundaries.py
```

[Offline Tooling](../../docs/OFFLINE_TOOLING.md#asset-authoring) owns clip
metadata, source/export commands, dependencies and the shared worker budget.
The committed GLB is authoritative where later geometry or clips extend a
legacy constructor. Compare a scratch export before overwriting those assets.
Beach builders provide their own deterministic `--check`; explicit
`--author-textures` changes their PNG sources. Contact sweeps establish
geometric behavior; native playback and lighting remain separate validation.
