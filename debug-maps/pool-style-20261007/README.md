# Pool concept refinement review copy — 2026-10-07

The committed visual journal is [docs/style-upgrade-20261007/pool](../../docs/style-upgrade-20261007/pool/README.md).
This directory preserves the local native review payload outside `target`.

`evidence/before` freezes the original `295ab2f` renderer, compiled demo,
catalog and all referenced asset families. `evidence/after` freezes the final
renderer, compiler, demo and assets. These are independent copies, without
symlinks into mutable repository artwork. `evidence/validation` preserves the
build, geometry, lint, test and capture logs. The payload is intentionally
ignored; matched PNGs and the public audit summaries are committed in `docs`.

Run a retained copy from the repository root with:

```sh
PLACES_ASSET_ROOT="$PWD/debug-maps/pool-style-20261007/evidence/after" \
PLACES_STATE_ROOT="$PWD/debug-maps/pool-style-20261007/evidence/after/state" \
PLACES_LEVEL=places_demo \
DYLD_LIBRARY_PATH="$PWD/debug-maps/pool-style-20261007/evidence/after/bin" \
debug-maps/pool-style-20261007/evidence/after/bin/places
```

Use `before` in each path to open the original. State files live beside each
copy. Asset and executable hashes are recorded in the retained manifests.
Neither historical debug-map evidence nor the external Consolidation/Office
evidence was modified. Reusable Cargo outputs remain in `target` for the next
serial pass.
