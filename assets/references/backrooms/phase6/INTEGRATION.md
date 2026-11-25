# Utility and HVAC kit integration

The 25 GLBs and native 256-square GIMP atlas are the original cloud Blender
4.3.2/GIMP 3.0.4 exports. The [editable Blender and GIMP masters](sources/)
and the [1024-square master texture](textures/) retain the original sibling
layout. No models were remade on the Razer.

The original [manifest](manifest.json), [source archive manifest](SOURCE_ARCHIVE_MANIFEST.json) and [transfer receipt](transfer_receipt.json) preserve the exact source hashes, original
transfer paths and public Git blob records. These are historical receipts,
not required authoring steps.

The [runtime GLBs and native atlas](../../../environments/backrooms/props/)
use the Backrooms catalog theme and carry the utility-kit tag. The current
catalog adds placement guidance and automatic duct tags.

## Editor placement

Use Props / scene objects > Static prop (catalog), then Configure > model to
choose a model. Search the Backrooms theme or the model name. Catalog size and
solid defaults follow model changes. Pose edits all three position and rotation
axes as one validated undo step. Prop Y is relative to the floor at its origin.

Select any of the six HVAC models and choose Duct joins in Simple mode, or Duct
in the Advanced inspector. Green marks end A and pink marks B. Arrows point out
of the end. Choose the source end, then a cardinal horizontal direction or
Up/Down to preview an orientation. Use Pose for arbitrary pitch/roll and height.
The target chooser offers up to 32 nearest visible fixed duct ends. Preview join
aligns the two measured outer flange faces, opposing their normals and matching
the rectangular aperture orientation. Apply creates one ordinary pose edit;
Cancel restores the document view. These are explicit placement aids, not a
permanent constraint or automatic resize system. Ducts must have matching scale;
fitted spans, floating props and moving physics poses cannot be joined this way.
[The editor duct metadata](../../../../src/editor/duct_join_metadata.json) derives the join face from actual final GLB bounds,
including projecting elbow flange stock, while preserving original anchors in
[the original connector receipt](hvac_connectors.json).

The ducts are decorative 0.6 x 0.4 m stock, separate from player crouch tunnels.
Their ordinary solid collision uses the conservative transformed catalog box;
the visible hollow aperture is not a player passage. Ceiling grids and passive
fixture props default nonsolid. Ceiling stock uses its measured top for manual
Pose mounting: local Y = ceiling Y - floor Y - model height. Wall clocks, speakers
and breaker panels use their measured rear bound against the wall.

## Operational fixtures and passive props

Add Light offers five `backrooms:light_fluorescent_*` entries. These render the
delivered housings and use the existing mount, Circuits, switches, flicker and
Full lighting tools. Their measured mounting offsets put each model's actual
top at the ceiling, including hanging anchors and the damaged model's displaced
origin. Source planes lie just below the actual lens or tube. Only material slot
2 (diffuser or intact tubes) emits; the body, label and damaged lens fragments
stay passive. Picking, prepared model placement and mounting use the same offset.
Each catalog light owns an exact existing 2:1 fluorescent PNG fallback face;
normal rendering uses the GLB's original embedded utility atlas. These fallback
copies preserve existing pixels and the fixture-face contract.

The same five entries under Props are explicitly labeled Passive Model and emit
no light. Use one operational housing instead of overlapping both versions.
The single and wired-glass double doors are explicitly Static Model scenery.
Use Add Door for an interactive leaf in an authored opening. The wall clock is
the fixed authored 10:08 pose and the wall speaker has no unsolicited sound.
The existing janitor cart, supply box, bottles, mop bucket and mop are reused.

## Audio and budget

Only fixed props tagged `hvac-duct` own the automatic `ambience_hvac` bed. One
nearest transformed duct envelope supplies one spatial voice for the family,
with 0.75 m near distance, squared falloff and an 8 m cutoff. Adjacent or duplicate
pieces cannot stack hum. Removal and map transitions clear the source. Old
authored global requests for this reserved cue cannot reintroduce a room-wide
bed. The authoring viewport remains silent; Play uses the runtime owner.

The atlas adds 0.25 MiB unique decoded RGBA, or 6.25 MiB when its embedded copy is
summed over 25 models. The pack audit reports 89.434 MiB by exact PNG identity,
conservatively above decoded-pixel sharing. The authorized desktop cap is now
128 MiB, retaining per-image and geometry safeguards without downscaling art.

The [historical non-release showcase](https://github.com/csd113/Places/blob/83218e22c1959872d0d54acb7e3c0bcf91185f5a/tools/props/backrooms_phase6/showcase/places.project.json)
records the original kit gallery, connected six-piece run, switched
fittings and reused maintenance stock. Builds and captures belong in
local verification storage, never beside source assets.
