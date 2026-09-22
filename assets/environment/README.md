# Environment assets

`environment/` groups content that furnishes an environment. Each theme gets a
directory; a theme is an organizational collection, never a placement rule:

* `office/` — the maintained and water-damaged liminal office set: wallpaper,
  carpet, ceiling panels, the fluorescent fixture and office furniture.
* `pool/` — the Pool environment family: deck, basin and wall tile, the sterile
  ceiling, the round and wall-mounted light fixtures, the white patio table and
  chair, the modular privacy curtains, the pool ladder, the modular silver
  guardrails and the final `NO DIVING` sign sheet.
* `decals/` — a file-backed decal sheet is external artwork exactly like a
  surface texture: the catalog entry carries the PNG path, the level names the
  decal id, and replacing the PNG needs no code change. The generated decal
  atlas is only used by the architecture diagnostics.

Any environment asset may be placed in any level. Office assets work in a Pool
level, Pool assets work in an Office level, and generic assets (catalogued
without a theme, physically under `../core/`) work everywhere.

See [`../README.md`](../README.md) for the catalog format and resolution flow.
