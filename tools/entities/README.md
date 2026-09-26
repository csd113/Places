# Entity asset toolkit

Development-only tooling that generates the shipped **entity** assets: rigged,
animated GLBs with a real skin, named joint hierarchy and named clips. The
static prop pack lives in [`../props/`](../props/README.md); this directory is
its skinned sibling.

| file | purpose |
| --- | --- |
| `rig.py` | rigged-GLB writer: joints, inverse bind matrices, `JOINTS_0`/`WEIGHTS_0`, LINEAR clips, `asset.extras.places_entity_clips` metadata, structural checker |
| `build_mannequin.py` | grey concrete human mannequin with three selectable poses |
| `build_rat.py` | animated rat: idle, walk and run |
| `build_skeleton.py` | articulated human skeleton prop with floor and chair sits |
| `validate_entities.py` | parallel per-frame skinning/contact sweep over built GLBs (`--workers N`, `PLACES_TOOL_WORKERS`) |
| `render_contact_sheets.py` | Blender reimport contact sheets (front/side/three-quarter per clip/pose), one bounded Blender process per asset |

## Conventions

Entities follow the prop conventions in [`../../assets/README.md`](../../assets/README.md):
1 unit = 1 metre, `+Y` up, `+Z` is the front at `rotation_degrees = 0`, the
origin is the floor-contact point horizontally centred under the bounding box.
Geometry is authored with `tools/props/mesh.py` primitives and painted with
`tools/props/tex.py`; the texture is embedded in the GLB (its source PNG is
kept beside the model under `assets/entities/<id>/textures/`). Model textures
are 128×128 or 256×256, opaque, non-tiling.

Each model ships one mesh, one primitive, one material, one embedded PNG, one
skin (≤ 128 joints) and a handful of clips (≤ 64). Weights are assigned by a
deterministic nearest-segment blend (`rig.auto_weights`), with explicit
overrides for rigid parts (skull, hands, feet); every vertex carries 1..4
normalised influences.

## Clip metadata

`rig.write_model` records `asset.extras.places_entity_clips`:

```json
{ "version": 2, "generator": "tools/entities/rig.py",
  "clips": [
    { "name": "walk", "duration": 0.45, "loop": true,
      "reference_speed_mps": 0.35, "kind": "walk" },
    { "name": "pose_arms_up", "duration": 0.0, "loop": true,
      "reference_speed_mps": null, "kind": "pose" }
  ] }
```

The Rust importer applies the marker per clip
(`PropAnimation::{looped, reference_speed_mps, kind}`, tolerant of a missing
marker). `reference_speed_mps` is the ground speed the clip's stride is
authored for: the runtime plays a route's walk/run clip at
`speed / reference_speed`, so a route authored at the same speed has no foot
sliding. A `duration` of `0` is a single-key hold pose.

## Commands

```sh
cd /path/to/Places
python3 tools/entities/build_rat.py                # write the asset + checks
python3 tools/entities/build_mannequin.py
python3 tools/entities/build_skeleton.py
python3 tools/entities/rig.py --check <glb>        # structural check only
python3 tools/entities/validate_entities.py --workers 8
python3 tools/entities/validate_entities.py --workers 1   # serial reference
```

`validate_entities.py` is the expensive offline check: it re-reads each built
GLB, evaluates every clip at a fine time sampling with the runtime's blend
maths (`p_posed = Σ w·(global_j(t)·IBM_j)·p_bind`, LINEAR), and reports floor
contact, deformation bounds and per-clip speed consistency. It uses a
`spawn` process pool by default, an importable worker function, a guarded
entry point, bounded chunks and deterministic result merging; `--workers 1`
runs the identical serial reference. `PLACES_TOOL_WORKERS` sets the default,
the CLI flag wins, and the effective worker count is reduced by the
independent task count, the `min(12, usable CPUs)` ceiling and a
per-worker memory guard (every reduction is printed with its reason).

## Adding an entity

1. Add `tools/entities/build_<id>.py` using `rig.py` (see `build_rat.py` for
   the smallest complete example).
2. Register the asset in `assets/catalog.json` (`asset_class: "entity"`,
   `asset_type: "entity"`, `entity_type: "character"`, `size` matching the
   bind-pose bounding box) and document it in
   `assets/entities/<id>/README.md` with clip/reference-speed and placement
   data.
3. `python3 tools/entities/build_<id>.py && python3 tools/entities/rig.py
   --check <glb> && python3 tools/entities/validate_entities.py` and then run
   the workspace checks.
