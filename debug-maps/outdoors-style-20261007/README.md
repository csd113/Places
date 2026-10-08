# Outdoors native review archive — 2026-10-07

The tracked [journal](../../docs/style-upgrade-20261007/outdoors/README.md) holds the actual before/after PNGs and review evidence. The ignored `evidence/` payload remains locally available outside `target`; do not clean it or older archives during the serial queue.

* `before/`: complete starting assets at `7ecacdfbe9f9c11df2c00932248a73634e78904e`, copied release game/compiler binaries, ten matched High and ten matched Low native views, settings, logs, timings and original package identities. High was captured before edits; Low was captured later from this immutable start copy.
* `candidate/`: first replacement assets and matched native views, retained to show the material/self-occlusion issue found during iteration.
* `after-visual-iteration/`: preserved complete preceding visual iteration, matching assets/packages/binaries and twenty native views.
* `after-order-iteration/`: complete corrected-order assets and matching binaries, retained before the static-fire repair; this stage was not captured.
* `after-static-iteration/`: preceding static-fire assets and matching binaries, with twenty native views before the final clapboard quadrant refinement.
* `after/`: final matching assets/packages and copied release binaries; ten matched High and ten matched Low native views.
* `validation/`: compile/verify/geometry, Python/Rust checks, scope hashes and retained intermediate failures. `asset-audit-final.json` is the full integrity inventory.

To play a retained copy from the repository root (replace `after` with `before` if desired):

```sh
PLACES_ASSET_ROOT="$PWD/debug-maps/outdoors-style-20261007/evidence/after" PLACES_STATE_ROOT=/tmp/places-outdoors-review PLACES_LEVEL=places_demo debug-maps/outdoors-style-20261007/evidence/after/bin/places
```

Native captures use `tools/bench/capture_outdoors.py`; its cameras and settings are frozen for this comparison. Production imagery is never generated at runtime. The detached inherited sleep assertion remains owned by the serial queue and is handed onward separately; persistent power/security settings are unchanged.
