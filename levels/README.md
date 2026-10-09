# Drop-in maps

This directory is for user-created and external **compiled** maps. Every
`*.placesmap` file placed here is discovered at startup and appears in the
Level Select menu next to the bundled levels.

A `.placesmap` is produced by the offline compiler from an authoring JSON
source. The player never compiles a map:

```sh
# from the repository root, with a release compiler built
./target/release/places-compile build my_level.json
# -> my_level.placesmap beside the source
```

Copy the resulting `.placesmap` here, or use the in-game **Import** action,
which accepts `.placesmap` files from `import/` and rejects raw `.json`/`.zip`
sources with the exact compiler command to run.

Bundled content lives in `assets/levels/` (read-only) and is packaged by
`tools/package.sh`. Authoring sources such as `assets/levels/places_demo.json`
are kept for authors but are never discovered as playable rows.

See `docs/PACKAGE_FORMAT.md` for the record contract and
`docs/MAP_AUTHORING_GUIDE.md` for the authoring workflow. `tools/package.sh`
copies compiled packages present here into an export; it never packages these
raw sources. Ignored local packages are absent from a fresh checkout.

The historical `home_showcase.json`, `geometry_intentional.json` and
`level0_pit.json` remain valid local controls. In particular, the local Home
source retains the legacy core furniture; the separate fixture Home source
exercises the completed Home kit. `blizzard_review.json` and
`snowfall_contrast.json` are recovered serialized LevelDef sources, extracted
byte for byte from their original package semantics so the reviews can be
rebuilt through the normal compiler. These local sources and packages remain
ignored. Their identities, provenance and individual commands are recorded in
`docs/art-style/stage7/content-inventory.json`.
