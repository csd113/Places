# Maintained maps

These authored sources are the supported playable lineup. `lineup.txt` is the
compiler/export allowlist; add a source and update that list deliberately.
The player discovers compiled packages in `assets/levels/`, while user imports
live in the writable state root's `levels/`. JSON is authoring input, never a
playable menu row. All `.placesmap` output is ignored by Git.

| Source / ID | Purpose | Source owner |
| --- | --- | --- |
| `places_demo` | Official combined Office, Pool, Home and night route | Authored source plus `build_outdoor_route.py`, `refine_home.py`, `refine_pool.py` |
| `model_zoo` | Every registered placeable, poses, routes and interactions | `build_model_zoo.py` |
| `pool_showcase` | Standalone Pool kit, lowered basin, fixtures, collision and signage | Authored source |
| `home_showcase` | Standalone Home rooms and generic architectural pieces | Authored source |
| `lantern_hollow` | Outdoor night village, forest, pond and encounters | `build_lantern_hollow.py` |
| `winter` | Snow village, lodge, aurora, ice traction and weather controls | `build_winter.py` |
| `beach_demo` | Coastal settlement, shoreline and animals | `build_beach.py` |
| `frutiger_aero_demo` | Day atrium, glass, water, fitted signage and interactive props | `build_frutiger_aero.py` |
| `retro_arcade_demo` | Cosmic bowling lanes, six reactive carpets, period arcade and fixed seating | `build_retro_arcade.py` |
| `office_demo` | Three connected Office storeys, a basement wanderer, rare hiding insects and failing lights | `build_office.py` |

Generators live under `tools/levels/`; change a generator rather than its
owned JSON. `--check` is read-only. Home and Pool are existing compact
showcases; current detailed refinements also appear in Places Demo. The Office
surface review under tests remains a diagnostic fixture; `office_demo` is the
playable three-floor environment.

```sh
sh tools/build-maps.sh --workers 8
python3 tools/levels/compile.py --check
PLACES_LEVEL=model_zoo ./target/release/places
```

Build only one changed source with an explicit runtime output path:

```sh
./target/release/places-compile build levels/winter.json \
  --out assets/levels/winter.placesmap --asset-root assets --workers 8
```

[Regression fixtures](../tests/fixtures/levels/README.md) retain focused
movement, geometry, entity, lighting and capacity controls without entering
this lineup. Superseded local review sources and packages are recoverable in
ignored `local/retained/`; fresh checkouts require none of them.
