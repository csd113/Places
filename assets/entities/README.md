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
them with the ordinary `props` format and nothing about their migration changes
the placement, orientation, scale or appearance.

The directory is designed to hold future player models, NPCs and creatures as
new subdirectories (`entities/<id>/model/...`) with catalog entries of their
own. Entity gameplay — AI, animation, player-character switching — is
deliberately not part of this architecture yet.

See [`../README.md`](../README.md).
