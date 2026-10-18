# Maintained map distributions

`lineup.txt` is the explicit distribution allowlist. The environment collection
has one editable project, `projects/environments/`, and one generated package,
`assets/levels/environments.placesmap`. Its entry map is the Central Hub; doors
connect all eight environments and each environment has a return door.

| Project level ID | Environment |
| --- | --- |
| `hub` | Central Hub |
| `home` | Home / Quiet Rooms |
| `pool` | Pool / Still Water |
| `office` | Office / After Hours |
| `lantern_hollow` | Lantern Hollow |
| `winter` | Winter / Snow & Shelter |
| `beach` | Beach / Beyond the Shore |
| `aero` | Frutiger Aero |
| `arcade` | Cosmic Lanes & Arcade |

The project manifest, level JSON and Luau modules are maintained editor sources.
Edit them through the builder/editor and preserve per-placement diagnostic
intent. The old environment generators are historical reconstruction tools;
their output does not own or regenerate the collection. They require an explicit
scratch destination. Places Demo and Model Zoo remain frozen historical JSON
under `docs/archive/levels/`. Relay Annex, Closing Time, Beam and unrelated
projects retain their separate purposes.

```sh
sh tools/build-maps.sh --workers 8
python3 tools/levels/compile.py --check
PLACES_LEVEL=hub ./target/release/places
```

`build-maps.sh` uses `export-project`, retaining every sub-map, declared script,
thumbnail and exact editable source in one content-addressed bundle. Verification
checks every sub-map's provenance, archive validity and Static/Full payloads;
`--check` performs those checks without rebuilding. `--map environments` selects
the distribution and still exports the complete project.

For an explicit project export or native compiler selection:

```sh
./target/release/places-compile export-project projects/environments \
  --out assets/levels/environments.placesmap --asset-root assets --workers 8
./target/release/places-compile verify projects/environments --level office \
  --package assets/levels/environments.placesmap --asset-root assets --require-current
```

The player discovers compiled packages in `assets/levels/`; user imports live
in the writable state root's `levels/`. JSON is authoring input. All `.placesmap`
output is ignored by Git. [Regression fixtures](../tests/fixtures/levels/README.md)
retain focused movement, geometry, entity, lighting and capacity controls outside
the distribution; ordinary CI never bakes a full map.
