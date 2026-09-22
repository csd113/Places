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

## Migration note

Goal 3 moved the model here from `assets/props/models/spooner-man.glb`. The
file bytes are unchanged (the move is recorded as a rename; the prop toolkit's
regenerated preview matches the committed thumbnail), and there is exactly one
copy of the resource in the repository.

The catalog maps the unchanged logical id `spooner-man` to
`entities/spooner-man/model/spooner-man.glb`, so levels that reference
`"model": "spooner-man"` load exactly as before. There is no alias, no
duplicate GLB and no second resolution path.

The mesh, texture, materials, proportions, origin, rotation and placement
behaviour were not touched. Regenerating the model is still possible with
`python3 tools/generate_spooner_man.py` (the generator writes to the catalog
path).
