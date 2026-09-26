# Spooner-Man (entity)

Spooner-Man is a low-poly tuxedo cat and the first entity asset.

| field | value |
| --- | --- |
| logical id | `spooner-man` |
| class | `entity` |
| type | `entity` |
| entity type | `character` |
| theme | none (entities belong to no environment theme) |
| canonical resource | `model/spooner-man.glb` |

## Model

The committed GLB is the **canonical** asset: a hand-authored Blender export
with one `cat_rig` skin (26 joints), three primitives and three embedded 256x256
textures. The engine bakes the bind pose into the ordinary static prop batch (so
the model occludes baked light and passes the shipped-asset checks like any
other prop) and re-poses every placed instance through the character path.

### Animation clips

`tools/props/animate_spooner_man.py` authors five LINEAR clips into the GLB
(it preserves the mesh, textures, hierarchy, skin and rest pose byte for byte,
appending only new buffer views):

| clip | duration | kind | use |
| --- | --- | --- | --- |
| `idle` | 4.0 s | loop | standing idle: breathing and tail sway with planted paws |
| `walk` | 0.6 s | loop | slow diagonal-pair gait, one stride per 0.120 m |
| `sit_down` | 1.6 s | once | stand to sit, holds the seated pose |
| `sit_idle` | 5.0 s | loop | seated pose with restrained breathing and planted front paws |
| `stand_up` | 1.4 s | once | sit back to stand |

The walk stride was measured from the rig, so the clip plays at rate 1.0 at
0.20 m/s and scales to the entity's route speed (0.26 m/s). Re-run the tool
after any rig change and `--check` to verify the clip table:

```sh
python3 tools/props/animate_spooner_man.py --report
python3 tools/props/animate_spooner_man.py --check
```

### Skin repair (run 3)

The original hand-authored export bound the paw geometry to the body chain
(measured: under 2% of the vertex weight reached any leg bone, and Blender's
own importer agreed that the lowest paw vertices were weighted
`chest`/`neck`), so no leg pose could deform the feet. The clip tool detects
that and rebinds every vertex with a deterministic nearest-segment two-bone
blend (`sigma = 0.02 m`, the central `root` bone excluded); the leg weight
share rises from 1.5% to 42.3%. Only the `JOINTS_0`/`WEIGHTS_0` bytes change:
positions, UVs, textures, the node hierarchy, the joint list, the inverse bind
matrices and the rest pose are untouched, and the bind pose renders exactly as
before. `--repair-skin` forces the rebind; without it the tool repairs only
weights that look degenerate.

The asset moved here from `assets/props/models/spooner-man.glb`; there is
exactly one copy of the resource in the repository, and the catalog maps the
logical id `spooner-man` to `entities/spooner-man/model/spooner-man.glb`, so
levels that reference `"model": "spooner-man"` load as they always have.

## Tooling

`python3 tools/props/generate_spooner_man.py` builds the toolkit's *static*
primitive cat — an earlier variant of this character, useful as a placeholder.
It refuses to overwrite the committed hand-authored model (its embedded skins or
animations protect it), and the same guard applies to a full
`python3 tools/props/build.py`; pass `--force` only when intentionally replacing
the rig with the static build.
