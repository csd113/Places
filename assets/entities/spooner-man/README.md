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
textures. It ships **no animation clips**. The engine bakes the bind pose into
the ordinary static prop batch (so the model occludes baked light and passes the
shipped-asset checks like any other prop) and re-poses every placed instance
through the character path from the player's locomotion state; the no-clip
procedural locomotion driver supplies the idle, walking, airborne and swimming
poses. See [`../README.md`](../README.md) for the engine side.

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
