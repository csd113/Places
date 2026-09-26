# Entities

Entity assets are characters and creatures. An entity is an **asset class**, not
an environment theme and not a prop: there is no `entities` theme anywhere in the
catalog.

```
entities/
  spooner-man/
    model/spooner-man.glb
```

* Logical id: `spooner-man` (unchanged; existing levels still reference it).
* `asset_class: entity`, `asset_type: entity`, `entity_type: character`.
* No theme: an entity may stand in any environment.

Entities resolve through the same placeable lookup as props, so levels place
them with the ordinary `props` format, and placement, orientation, scale and
appearance are unchanged.

`spooner-man.glb` is a hand-exported Blender model with a real skin: one
`cat_rig` skin, 26 joints (`root`, `pelvis`, `spine`/`chest`/`neck`/`head`, four
`leg_*` chains and `tail_01`..`tail_08`) and three primitives, and it ships
**no animation clips**. The engine bakes the bind pose into the ordinary static
prop batch, so the model still occludes baked light and passes the shipped-asset
checks like any other prop; at run time the character path claims every placed
skinned model and re-poses it on the CPU from the player's locomotion state
(idle, walking, airborne, swimming). With no clips to play, the animator uses
its procedural locomotion driver — a diagonal leg gait, a tail sway, a
breathing body chain and state-specific poses — and stays in the rest pose for
a rig it cannot classify. A rig that ships `idle`, `walk`/`run`, `jump`/`air`
or `swim` clips instead plays those, crossfading between states.

The directory is designed to hold future player models, NPCs and creatures as
new subdirectories (`entities/<id>/model/...`) with catalog entries of their
own. Entity gameplay — AI, player-character switching — is deliberately not
part of this architecture yet.

See [`../README.md`](../README.md).
