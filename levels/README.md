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
`docs/MAP_AUTHORING_GUIDE.md` for the authoring workflow. Nothing in this
directory is part of the game's packaged content: `tools/package.sh` copies the
packages that are already here, so a fresh checkout and a packaged build both
start with the bundled levels.
