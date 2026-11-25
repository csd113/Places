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
Edit them through the native editor and preserve per-placement diagnostic
intent. The retired environment generators do not own the collection. Places Demo and Model Zoo remain frozen historical JSON
under [`archive/levels/`](../archive/levels/). Beam remains active in the broader release inventory.
Six retired projects retain complete editable content in
[`archive/projects/`](../archive/projects/README.md). Development controls stay
outside release discovery; their retained purposes are documented there too.

In the editor, open the project and use **Project > Export all levels + sources**
to prepare an updated distribution. The export retains every sub-map, declared
script, thumbnail and exact editable source. To check an existing export without
rebaking, validate it against the current assets and verify every project member:

```sh
./target/release/places-map-compiler validate assets/levels/environments.placesmap --asset-root assets
./target/release/places-map-compiler verify-project projects/environments \
  --package assets/levels/environments.placesmap --asset-root assets --require-current
PLACES_LEVEL=hub ./target/release/places
```

The [retirement handoff](https://github.com/csd113/Places/blob/29e12d9aa52b6c54f1d5d2d46cf8a524cf5862f7/docs/reports/release-0.12.0.md#environment-collection-0120-retirement)
lists all eight superseded standalone IDs and the preserved recovery material.

For an explicit project export or native compiler selection:

```sh
./target/release/places-map-compiler export-project projects/environments \
  --out assets/levels/environments.placesmap --asset-root assets --workers 8
./target/release/places-map-compiler verify projects/environments --level office \
  --package assets/levels/environments.placesmap --asset-root assets --require-current
```

The player discovers compiled packages in `assets/levels/`; user imports live
in the writable state root's `levels/`. JSON is authoring input. All `.placesmap`
output is ignored by Git. [Regression fixtures](../tests/fixtures/levels/README.md)
retain focused movement, geometry, entity, lighting and capacity controls outside
the distribution; ordinary CI never bakes a full map.
