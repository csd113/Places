# Engine regression scenes

These are functional test inputs, outside the playable lineup and distribution.
Tests name them directly; generated scenes have `--check` owners under
`tools/levels/`. Preserve assertions when changing a fixture.

- `movement_test`, `level0_pit`, `vertical_diagnostic`: controller steps,
  slopes, jumps, water, ladders, swept reset triggers and height limits.
- `geometry_*`, `repair/`, `invalid/`: checker positives, planted errors and
  loader-negative controls. `invalid/geometry_invalid` must fail compilation.
- `prop_showcase`, `prop_stress`, `entity_showcase`, `halloween_entities`:
  geometry, batching, collision, clip/pose/routes and component behavior.
- `capacity_dense`, `capacity_beyond_former_limits`: count, batching and u32
  material boundaries. The synthetic-ID boundary witness is CPU-only.
- `lighting_*`, `water_transmission`, `rendering_diagnostic`,
  `fog_void_validation`, `round_pool_and_proximity_fade`: named transport,
  isolation, decals, fog, water and alpha controls.
- `material_transport`, `architecture_before`, `architecture_after`: compact
  extracted controls for source colour, glass coverage, dependency closure,
  unchanged traversal and passable decorative refinements.
- `office_asset_review`, `outdoor_kit_showcase`: focused asset-family QA.

Home and Pool showcases now live in `levels/` and serve both developers and
tests. Obsolete full Hero stage variants and the asset-polish review have been
retired; their surviving contracts are covered by the focused scenes above.
Native camera inputs are under `tests/fixtures/native/map-manifests/` and must
match the maintained source inventory exactly. Capture output is ignored.

Compile a positive fixture to isolated state to inspect it:

```sh
mkdir -p /tmp/places-fixture/levels
./target/release/places-compile build tests/fixtures/levels/test_room.json \
  --out /tmp/places-fixture/levels/test_room.placesmap --asset-root assets
PLACES_STATE_ROOT=/tmp/places-fixture PLACES_LEVEL=test_room ./target/release/places
```
