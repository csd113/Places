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

`tools/props/animate_spooner_man.py` and `tools/props/cat_motion.py` author
seven LINEAR clips into the canonical GLB. Mesh, textures, hierarchy, weights,
inverse bind matrices, rest pose and the standing idle are preserved.

| clip | duration | kind | use |
| --- | --- | --- | --- |
| `idle` | 4.0 s | loop | original standing breathing and tail sway |
| `walk` | 0.6 s | loop | four-beat lateral-sequence cat walk, low paw lift |
| `run` | 0.46 s | loop | hind push-off followed by offset foreleg catches |
| `sit_down` | 1.8 s | once | lowers onto folded haunches while forepaws brace |
| `sit_idle` | 5.0 s | loop | seated breathing, folded hocks, tail swept aside |
| `stand_up` | 1.5 s | once | rises from the same seated pose |
| `pounce` | 1.4 s | once | anticipation crouch, launch, airborne reach, foreleg landing, recovery |

The four-beat walk places hind-left, front-left, hind-right, front-right in
sequence. It has 68% stance duty and a 0.20 m/s reference speed. The run uses
36% stance duty and a 0.60 m/s reference speed. Per-clip metadata stores those
speeds, loop flags and durations; the current character renderer selects the
run for faster `move_to` movement and scales playback to the requested speed.

These are **in-place** animations: no horizontal root displacement. The pounce
has a vertical pelvis arc; it does not propel the entity through the level or
add attack behavior. Play it through the existing named-clip route step:

```json
{ "step": "play", "clip": "pounce", "seconds": 1.4, "loop": false }
```

Two-link leg IK preserves bone lengths and paw orientation. Transitions share
exact endpoints; all looping clips close exactly. Ground correction is checked
against the skinned mesh at every authored pose. The pounce starts and finishes
in the idle stance. No new rig, textures or skin-weight edits were needed.

```sh
python3 tools/props/animate_spooner_man.py            # rebuild clips only
python3 tools/props/animate_spooner_man.py --check
python3 tools/entities/check_clip_boundaries.py
python3 tools/entities/validate_entities.py --glb assets/entities/spooner-man/model/spooner-man.glb --workers 2
```

Motion previews, preservation hashes and the 60 Hz export sweep are documented
in `docs/reports/spoonerman-cat-motion.md`.

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
